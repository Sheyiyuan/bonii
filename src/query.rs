use crate::model::{Catalog, Fuzzy, Hit, Shape, Source};
use crate::normalize;
use crate::util::percent_encode_path;
use anyhow::Result;
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum Outcome {
    Taken { detail: String },
    Free,
    Skipped { reason: String },
    Error { reason: String },
}

#[derive(Debug, Clone)]
pub struct Row {
    pub label: String,
    pub queried: String,
    pub outcome: Outcome,
}

pub struct Client {
    inner: reqwest::Client,
    /// Per-request timeout. GitHub's `gh`/REST paths reuse it, and fuzzy
    /// scanning derives its total budget from it.
    pub timeout: Duration,
}

impl Client {
    pub fn new(catalog: &Catalog, timeout: Duration) -> Result<Self> {
        let inner = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent(catalog.user_agent.string.clone())
            .build()?;
        Ok(Self { inner, timeout })
    }

    async fn get(&self, url: &str) -> Result<(u16, String)> {
        let resp = self.inner.get(url).send().await?;
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        Ok((status, body))
    }
}

/// Some(reason) when the input has a shape this ecosystem cannot have, so the
/// answer would be a meaningless "free" instead of a real judgement.
pub fn input_reject(src: &Source, name: &str) -> Option<String> {
    let pathed = normalize::is_pathed(name);
    if !src.shape.accepts(pathed) {
        return Some(if pathed {
            "输入是带命名空间的标识，该生态不适用".into()
        } else {
            "该生态按带前缀的完整标识命名，裸名无法判定".into()
        });
    }
    if src.shape == Shape::Pathed && name.contains('@') {
        return Some("标识含 `@`，该生态没有这种形态".into());
    }
    if src.require_host {
        let host = name.split('/').next().unwrap_or("");
        if !host.contains('.') {
            return Some("Go 模块需带域名前缀，如 github.com/owner/name".into());
        }
    }
    None
}

/// One exact-name query against one source.
pub async fn query(client: &Client, src: &Source, name: &str) -> Outcome {
    if let Some(reason) = input_reject(src, name) {
        return Outcome::Skipped { reason };
    }
    let normalized = normalize::apply(src.normalize.as_deref(), name);
    let Some(template) = src.query.as_deref() else {
        return Outcome::Skipped {
            reason: "该源未配置查询端点".into(),
        };
    };
    let substituted = if src.encode {
        percent_encode_path(&normalized)
    } else {
        normalized.clone()
    };
    let url = template.replace("{name}", &substituted);

    let (status, body) = match client.get(&url).await {
        Ok(v) => v,
        Err(e) => {
            return Outcome::Error {
                reason: format!("请求失败: {e}"),
            };
        }
    };
    evaluate(src, status, &body)
}

fn evaluate(src: &Source, status: u16, body: &str) -> Outcome {
    if status == 429 {
        return Outcome::Error {
            reason: "被限流 (429)，稍后重试或降低并发".into(),
        };
    }
    let parsed: Option<Value> = serde_json::from_str(body).ok();

    let taken = match &src.hit {
        Hit::StatusOk => (200..300).contains(&status),
        _ if !(200..300).contains(&status) => false,
        Hit::Greater(n) => field_at_opt(&parsed, src.extract.as_deref())
            .and_then(Value::as_i64)
            .is_some_and(|v| v > *n),
        Hit::Nonempty => match field_at_opt(&parsed, src.extract.as_deref()) {
            Some(v) => nonempty(v),
            // body is not JSON: fall back to "did it return anything at all"
            None => !body.trim().is_empty(),
        },
    };

    if taken {
        Outcome::Taken {
            detail: summarize(src, &parsed, body),
        }
    } else {
        Outcome::Free
    }
}

/// Fetch AUR-style suggestions, then re-query each candidate against the
/// verifier endpoint. AUR's suggest index also lists removed packages, so a
/// bare hit list is not authoritative. Returns `(name, detail)` pairs; the
/// detail comes from the confirming exact query. The whole scan is capped at
/// `FUZZY_BUDGET_FACTOR ×` the per-request timeout so one slow endpoint cannot
/// stall the run.
pub async fn fuzzy(
    client: &Client,
    src: &Source,
    verifier: &Source,
    name: &str,
) -> Vec<(String, String)> {
    let Some(Fuzzy {
        endpoint, extract, ..
    }) = &src.fuzzy
    else {
        return Vec::new();
    };
    let normalized = normalize::apply(src.normalize.as_deref(), name);
    let url = endpoint.replace("{name}", &percent_encode_path(&normalized));
    let Ok((_, body)) = client.get(&url).await else {
        return Vec::new();
    };
    let Ok(root) = serde_json::from_str::<Value>(&body) else {
        return Vec::new();
    };
    let Some(list) =
        field_at(&root, extract.as_deref().unwrap_or("root")).and_then(Value::as_array)
    else {
        return Vec::new();
    };

    let budget = client.timeout.saturating_mul(FUZZY_BUDGET_FACTOR);
    let started = std::time::Instant::now();
    let mut confirmed = Vec::new();
    for candidate in list
        .iter()
        .filter_map(Value::as_str)
        .take(FUZZY_CANDIDATE_LIMIT)
    {
        if started.elapsed() >= budget {
            break;
        }
        if candidate == normalized {
            continue;
        }
        if let Outcome::Taken { detail } = query(client, verifier, candidate).await {
            confirmed.push((candidate.to_string(), detail));
        }
    }
    confirmed
}

const FUZZY_CANDIDATE_LIMIT: usize = 40;
const FUZZY_BUDGET_FACTOR: u32 = 4;

impl Shape {
    fn accepts(self, pathed: bool) -> bool {
        match self {
            Shape::Bare => !pathed,
            Shape::Pathed => pathed,
            Shape::Both => true,
        }
    }
}

fn field_at_opt<'a>(parsed: &'a Option<Value>, path: Option<&str>) -> Option<&'a Value> {
    match (parsed.as_ref(), path) {
        (Some(v), Some(p)) => field_at(v, p),
        (Some(v), None) => Some(v),
        _ => None,
    }
}

fn field_at<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = root;
    if path == "root" {
        return Some(cur);
    }
    for seg in path.split('.') {
        cur = match cur {
            Value::Object(map) => map.get(seg)?,
            Value::Array(items) => items.get(seg.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

fn summarize(src: &Source, parsed: &Option<Value>, body: &str) -> String {
    let parts: Vec<String> = src
        .info
        .iter()
        .filter_map(|path| {
            parsed
                .as_ref()
                .and_then(|v| field_at(v, path))
                .and_then(value_to_text)
        })
        .collect();
    if !parts.is_empty() {
        return parts.join(" — ");
    }
    if parsed.is_some() {
        return String::new();
    }
    // plain-text endpoints (e.g. Go proxy version lists) — summarise, don't dump
    let lines: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    match lines.first() {
        Some(first) if lines.len() > 1 => format!(
            "{} 条记录，首条 {}",
            lines.len(),
            crate::util::truncate(first, 24)
        ),
        Some(first) => crate::util::truncate(first, 60),
        None => String::new(),
    }
}

fn value_to_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn nonempty(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_i64().map(|i| i != 0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}
