use crate::query::Outcome;
use crate::util::{percent_encode, truncate};
use serde_json::Value;
use std::process::Stdio;
use std::time::Duration;

#[derive(Debug)]
pub struct Repo {
    pub full_name: String,
    pub stars: i64,
    pub archived: bool,
    /// owner login starts with the queried name: usually a personal repo named
    /// after its user, i.e. username noise rather than a project claiming it
    pub self_named: bool,
}

/// GitHub has no "is this name taken" endpoint: search returns substring and
/// relevance matches, so the exact-name filter must run locally. Prefers the
/// authenticated `gh` CLI and degrades to anonymous REST when it is absent or
/// logged out.
///
/// Search API quotas are metered separately from the core REST quota:
/// 30 requests/min authenticated, 10 requests/min anonymous.
pub async fn check(name: &str, timeout: Duration) -> Outcome {
    if gh_usable(timeout).await {
        return gh_cli(name, timeout).await;
    }
    anonymous_rest(name, timeout).await
}

async fn gh_usable(timeout: Duration) -> bool {
    let mut cmd = tokio::process::Command::new("gh");
    cmd.args(["auth", "status"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    match tokio::time::timeout(timeout, cmd.status()).await {
        Ok(Ok(status)) => status.success(),
        _ => false,
    }
}

async fn gh_cli(name: &str, timeout: Duration) -> Outcome {
    let limit = SEARCH_LIMIT.to_string();
    let mut cmd = tokio::process::Command::new("gh");
    cmd.args([
        "search",
        "repos",
        "--match",
        "name",
        name,
        "--limit",
        limit.as_str(),
        "--json",
        "fullName,stargazersCount,isArchived",
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);

    let out = match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            return Outcome::Error {
                reason: format!("无法执行 gh: {e}"),
            };
        }
        Err(_) => {
            return Outcome::Error {
                reason: format!("gh search 超时（{}s）", timeout.as_secs()),
            };
        }
    };
    if !out.status.success() {
        return Outcome::Error {
            reason: format!(
                "gh search 失败: {}",
                truncate(String::from_utf8_lossy(&out.stderr).trim(), 120)
            ),
        };
    }
    // gh gives no total count, so a full page means more matches may exist
    outcome_from_json_paginated(&String::from_utf8_lossy(&out.stdout), name)
}

fn outcome_from_json_paginated(raw: &str, name: &str) -> Outcome {
    match serde_json::from_str::<Value>(raw) {
        Ok(v) => {
            let items = v.as_array().cloned().unwrap_or_default();
            let truncated = items.len() >= SEARCH_LIMIT;
            outcome_from_value(&items, name, truncated)
        }
        Err(_) => Outcome::Error {
            reason: "无法解析搜索结果".into(),
        },
    }
}

async fn anonymous_rest(name: &str, timeout: Duration) -> Outcome {
    let url = format!(
        "https://api.github.com/search/repositories?q={}+in:name&per_page={}",
        percent_encode(name),
        SEARCH_LIMIT
    );
    let client = match reqwest::Client::builder()
        .timeout(timeout)
        .user_agent("bonii/0.1 name availability checker")
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return Outcome::Error {
                reason: e.to_string(),
            };
        }
    };
    let Ok(resp) = client.get(&url).send().await else {
        return Outcome::Error {
            reason: "GitHub 匿名 REST 请求失败（未检测到 gh CLI，建议 `gh auth login`）".into(),
        };
    };
    let status = resp.status().as_u16();
    if status == 403 {
        return Outcome::Error {
            reason: "GitHub 匿名限流 (403)，请安装并登录 gh CLI".into(),
        };
    }
    let Ok(body) = resp.text().await else {
        return Outcome::Error {
            reason: "GitHub 响应读取失败".into(),
        };
    };
    let Ok(root) = serde_json::from_str::<Value>(&body) else {
        return Outcome::Error {
            reason: "GitHub 返回非 JSON".into(),
        };
    };
    let Some(items) = root.get("items").and_then(Value::as_array) else {
        return Outcome::Error {
            reason: "GitHub 响应缺少 items".into(),
        };
    };
    let total = root.get("total_count").and_then(Value::as_i64).unwrap_or(0);
    outcome_from_value(items, name, total > items.len() as i64)
}

fn outcome_from_value(items: &[Value], name: &str, truncated: bool) -> Outcome {
    let needle = name.to_lowercase();
    let mut exact: Vec<Repo> = items
        .iter()
        .filter_map(|r| {
            // gh --json emits camelCase, GitHub REST emits snake_case
            let full = pick(r, "fullName", "full_name")?.as_str()?;
            let (owner, repo) = full.split_once('/')?;
            if repo.to_lowercase() != needle {
                return None;
            }
            Some(Repo {
                full_name: full.to_string(),
                stars: pick(r, "stargazersCount", "stargazers_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
                archived: pick(r, "isArchived", "archived")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                self_named: owner.to_lowercase().starts_with(&needle),
            })
        })
        .collect();

    // A full page of substring matches proves nothing: the exact repo may sit
    // below the cut. Only a page we know we exhausted can yield a real "free".
    if exact.is_empty() {
        return if truncated {
            Outcome::Error {
                reason: format!(
                    "搜索结果已达上限（{SEARCH_LIMIT} 条）且未含精确同名仓库，精确匹配可能被挤出结果，无法确认可用"
                ),
            }
        } else {
            Outcome::Free
        };
    }
    exact.sort_by_key(|r| std::cmp::Reverse(r.stars));

    let self_named = exact.iter().filter(|r| r.self_named).count();
    let listed: Vec<String> = exact
        .iter()
        .take(5)
        .map(|r| {
            let mut flags = String::new();
            if r.archived {
                flags.push_str("，已归档");
            }
            if r.self_named {
                flags.push_str("，用户名派生");
            }
            format!("{}({}★{})", r.full_name, r.stars, flags)
        })
        .collect();
    let hidden = exact.len().saturating_sub(5);
    let more = if hidden > 0 {
        format!(" +{hidden}")
    } else {
        String::new()
    };
    let bound = if truncated { "至少 " } else { "" };
    let max_stars = exact.iter().map(|r| r.stars).max().unwrap_or(0);
    let mut notes = Vec::new();
    if self_named > 0 {
        notes.push(format!(
            "其中 {self_named} 个由用户名派生，属个人占位而非项目占用"
        ));
    }
    if max_stars == 0 {
        notes.push("全部 0★，多为临时/空仓".to_string());
    }
    let note = if notes.is_empty() {
        String::new()
    } else {
        format!("（{}）", notes.join("；"))
    };
    Outcome::Taken {
        detail: format!(
            "{bound}{} 个同名仓库: {}{more}{note}",
            exact.len(),
            listed.join(", ")
        ),
    }
}

const SEARCH_LIMIT: usize = 50;

fn pick<'a>(r: &'a Value, gh_key: &str, rest_key: &str) -> Option<&'a Value> {
    r.get(gh_key).or_else(|| r.get(rest_key))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome_text(o: &Outcome) -> String {
        match o {
            Outcome::Taken { detail } => detail.clone(),
            other => format!("{other:?}"),
        }
    }

    fn items(pairs: &[(&str, i64, bool)]) -> Vec<Value> {
        pairs
            .iter()
            .map(|(full, stars, archived)| {
                serde_json::json!({
                    "fullName": full, "stargazersCount": stars, "isArchived": archived,
                    "full_name": full, "stargazers_count": stars, "archived": archived,
                })
            })
            .collect()
    }

    #[test]
    fn substring_matches_are_filtered_out() {
        let got = outcome_from_value(&items(&[("acme/qoder-proxy", 900, false)]), "qoder", false);
        assert!(matches!(got, Outcome::Free), "{got:?}");
    }

    #[test]
    fn exact_match_is_case_insensitive_and_reported() {
        let got = outcome_from_value(&items(&[("acme/Qoder", 3, false)]), "qoder", false);
        assert!(matches!(got, Outcome::Taken { .. }), "{got:?}");
    }

    #[test]
    fn rest_snake_case_is_understood() {
        let raw = r#"[{"full_name":"acme/qoder","stargazers_count":7,"archived":true}]"#;
        let text = outcome_text(&outcome_from_json_paginated(raw, "qoder"));
        assert!(text.contains("1 个同名仓库"), "{text}");
        assert!(text.contains("已归档"), "{text}");
    }

    #[test]
    fn full_page_is_reported_as_a_lower_bound() {
        let many: Vec<Value> = (0..SEARCH_LIMIT)
            .map(|i| {
                serde_json::json!({"fullName":format!("u{i}/qoder"),"stargazersCount":i,"isArchived":false})
            })
            .collect();
        let text = outcome_text(&outcome_from_value(&many, "qoder", true));
        assert!(text.starts_with("至少 "), "{text}");
    }

    #[test]
    fn full_page_of_substring_matches_is_not_a_free_answer() {
        // 50 substring hits, no exact `qoder`: the exact repo may sit below the
        // cut, so this must be an error (incomplete), never "free".
        let many: Vec<Value> = (0..SEARCH_LIMIT)
            .map(|i| {
                serde_json::json!({"fullName":format!("u{i}/qoder-proxy"),"stargazersCount":i,"isArchived":false})
            })
            .collect();
        let got = outcome_from_value(&many, "qoder", true);
        assert!(matches!(got, Outcome::Error { .. }), "{got:?}");
    }

    #[test]
    fn exhausted_page_of_substring_matches_is_free() {
        let many: Vec<Value> = (0..3)
            .map(|i| {
                serde_json::json!({"fullName":format!("u{i}/qoder-proxy"),"stargazersCount":i,"isArchived":false})
            })
            .collect();
        let got = outcome_from_value(&many, "qoder", false);
        assert!(matches!(got, Outcome::Free), "{got:?}");
    }
}
