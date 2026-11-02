mod catalog;
mod github;
mod model;
mod normalize;
mod query;
mod report;
mod util;

use crate::catalog::load;
use crate::model::{Catalog, Kind, Source};
use crate::query::{Client, Outcome, Row};
use clap::Parser;
use std::io::IsTerminal;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;

#[derive(Parser)]
#[command(
    name = "bonii",
    version,
    about = "ボニー (bonii) — 检查项目/包名在 GitHub、各语言包管理仓库和发行版仓库中是否已被占用"
)]
struct Args {
    /// 待查询的名字；npm scope 可写成 @scope/name，Go 可写成完整模块路径
    name: String,

    /// 只查询指定源（可重复），默认全部
    #[arg(short = 's', long = "source")]
    sources: Vec<String>,

    /// 排除指定源（可重复）
    #[arg(long = "exclude")]
    exclude: Vec<String>,

    /// 不探测生态惯用后缀（AUR 的 -git/-bin 等）
    #[arg(long)]
    no_variants: bool,

    /// 附加相似名扫描（目前实现 AUR suggest，命中逐个复核存在性）
    #[arg(long)]
    fuzzy: bool,

    /// 输出 JSON 而非表格
    #[arg(long)]
    json: bool,

    /// 单请求（含 gh 调用）超时秒数
    #[arg(long, default_value_t = 15)]
    timeout: u64,

    /// 强制着色：auto|always|never
    #[arg(long, default_value = "auto")]
    color: String,
}

fn main() {
    let args = Args::parse();
    std::process::exit(run(args));
}

fn run(args: Args) -> i32 {
    let cat = match load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("内置源定义解析失败: {e:?}");
            return 2;
        }
    };
    let selected = match filter(&cat, &args) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("{msg}");
            return 2;
        }
    };
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("运行时初始化失败: {e}");
            return 2;
        }
    };

    let rows = match rt.block_on(execute(&cat, &cat_ids(&cat, &selected), &args)) {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("查询失败: {e}");
            return 3;
        }
    };
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json_rows(&rows)).unwrap_or_default()
        );
    } else {
        println!("{}", report::render(&rows, color_on(&args.color)));
    }

    if rows
        .iter()
        .any(|r| matches!(r.outcome, Outcome::Taken { .. }))
    {
        1
    } else if rows
        .iter()
        .any(|r| matches!(r.outcome, Outcome::Error { .. }))
    {
        3
    } else {
        0
    }
}

fn cat_ids(cat: &Catalog, selected: &[String]) -> Vec<String> {
    cat.source
        .iter()
        .map(|s| s.id.clone())
        .filter(|id| selected.contains(id))
        .collect()
}

async fn execute(cat: &Catalog, selected: &[String], args: &Args) -> anyhow::Result<Vec<Row>> {
    let client = Arc::new(Client::new(cat, Duration::from_secs(args.timeout))?);
    let name = args.name.trim();

    let mut set = JoinSet::new();
    let mut order: Vec<(String, String)> = Vec::new();

    for src in cat.source.iter().filter(|s| selected.contains(&s.id)) {
        let queried = normalize::apply(src.normalize.as_deref(), name);
        if let Some(reason) = query::input_reject(src, name) {
            order.push((src.id.clone(), queried.clone()));
            let job = Job {
                source_id: src.id.clone(),
                label: src.label.clone(),
                queried,
                outcome: Outcome::Skipped { reason },
            };
            set.spawn(async move { job });
            continue;
        }
        spawn_exact(&mut set, src, name, &client);
        order.push((src.id.clone(), queried));
        if !args.no_variants {
            for suffix in &src.variants {
                let variant = format!("{name}-{suffix}");
                spawn_exact(&mut set, src, &variant, &client);
                order.push((
                    src.id.clone(),
                    normalize::apply(src.normalize.as_deref(), &variant),
                ));
            }
        }
    }

    let mut got = Vec::new();
    while let Some(Ok(r)) = set.join_next().await {
        got.push(r);
    }

    // restore catalog order, grouping variant/fuzzy probes under their source
    let mut rows = Vec::new();
    for (id, queried) in &order {
        if let Some(r) = got
            .iter()
            .find(|r| &r.source_id == id && &r.queried == queried)
        {
            rows.push(r.to_row());
        }
    }

    if args.fuzzy {
        for src in cat
            .source
            .iter()
            .filter(|s| s.fuzzy.is_some() && selected.contains(&s.id))
        {
            let Some(fuzzy) = src.fuzzy.as_ref() else {
                continue;
            };
            let Some(verifier) = cat.source.iter().find(|v| v.id == fuzzy.verify) else {
                continue;
            };
            let label = fuzzy.label.as_deref().unwrap_or("近似");
            let note = fuzzy
                .note
                .as_deref()
                .unwrap_or("近似名已存在，易被误认或卷入 typosquat 争议");
            for (candidate, detail) in query::fuzzy(&client, src, verifier, name).await {
                let detail = if detail.is_empty() {
                    note.to_string()
                } else {
                    format!("{note} — {detail}")
                };
                rows.push(Row {
                    label: format!("{} ·{label}", src.label),
                    queried: candidate,
                    outcome: Outcome::Taken { detail },
                });
            }
        }
    }
    Ok(rows)
}

fn spawn_exact(set: &mut JoinSet<Job>, src: &Source, name: &str, client: &Arc<Client>) {
    let label = src.label.clone();
    let source_id = src.id.clone();
    let queried = normalize::apply(src.normalize.as_deref(), name);
    let github = src.kind() == Kind::Github;
    let src = src.clone();
    let name = name.to_string();
    let client = client.clone();
    set.spawn(async move {
        let timeout = client.timeout;
        let outcome = if github {
            github::check(&queried, timeout).await
        } else {
            query::query(&client, &src, &name).await
        };
        Job {
            source_id,
            label,
            queried,
            outcome,
        }
    });
}

struct Job {
    source_id: String,
    label: String,
    queried: String,
    outcome: Outcome,
}

impl Job {
    fn to_row(&self) -> Row {
        Row {
            label: self.label.clone(),
            queried: self.queried.clone(),
            outcome: self.outcome.clone(),
        }
    }
}

fn filter(cat: &Catalog, args: &Args) -> Result<Vec<String>, String> {
    let known: Vec<String> = cat.source.iter().map(|s| s.id.clone()).collect();
    let mut chosen = if args.sources.is_empty() {
        known.clone()
    } else {
        for s in &args.sources {
            if !known.iter().any(|k| k == s) {
                return Err(format!("未知源 `{s}`，可选: {}", known.join(", ")));
            }
        }
        args.sources.clone()
    };
    chosen.retain(|id| !args.exclude.iter().any(|e| e == id));
    if chosen.is_empty() {
        return Err("没有可查询的源".into());
    }
    Ok(chosen)
}

fn color_on(mode: &str) -> bool {
    match mode {
        "always" => true,
        "never" => false,
        _ => std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
    }
}

fn json_rows(rows: &[Row]) -> serde_json::Value {
    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let (status, detail) = match &r.outcome {
                Outcome::Taken { detail } => ("taken", detail.clone()),
                Outcome::Skipped { reason } => ("skipped", reason.clone()),
                Outcome::Error { reason } => ("error", reason.clone()),
                Outcome::Free => ("free", String::new()),
            };
            serde_json::json!({
                "source": r.label,
                "queried": r.queried,
                "status": status,
                "detail": detail,
            })
        })
        .collect();
    serde_json::json!({ "rows": items })
}
