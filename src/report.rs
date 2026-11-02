use crate::query::{Outcome, Row};
use crate::util::truncate;

pub fn render(rows: &[Row], color: bool) -> String {
    let status_cell = |o: &Outcome| match o {
        Outcome::Taken { .. } => pad("占用", 4, "\x1b[31m", color),
        Outcome::Free => pad("可用", 4, "\x1b[32m", color),
        Outcome::Skipped { .. } => pad("跳过", 4, "\x1b[33m", color),
        Outcome::Error { .. } => pad("错误", 4, "\x1b[35m", color),
    };
    let detail_of = |o: &Outcome| match o {
        Outcome::Taken { detail } => detail.clone(),
        Outcome::Skipped { reason } => reason.clone(),
        Outcome::Error { reason } => reason.clone(),
        Outcome::Free => String::new(),
    };

    let label_w = rows
        .iter()
        .map(|r| r.label.chars().count())
        .max()
        .unwrap_or(0);
    let query_w = rows
        .iter()
        .map(|r| r.queried.chars().count())
        .max()
        .unwrap_or(0);

    let mut out = String::new();
    for r in rows {
        out.push_str(&format!(
            "{}  {}  {}  {}\n",
            spaces(&r.label, label_w),
            spaces(&r.queried, query_w),
            status_cell(&r.outcome),
            truncate(&detail_of(&r.outcome), 120)
        ));
    }

    let mut counts = [0usize; 4];
    for r in rows {
        counts[cell_of(&r.outcome)] += 1;
    }
    let [taken, free, skipped, errors] = counts;
    out.push_str(&format!(
        "\n占用 {taken} · 可用 {free} · 跳过 {skipped} · 错误 {errors}"
    ));
    out
}

fn cell_of(o: &Outcome) -> usize {
    match o {
        Outcome::Taken { .. } => 0,
        Outcome::Free => 1,
        Outcome::Skipped { .. } => 2,
        Outcome::Error { .. } => 3,
    }
}

/// Pad by *visible* width, then wrap in color — padding an escape-annotated
/// string would count the control bytes as width.
fn pad(text: &str, width: usize, code: &str, color: bool) -> String {
    let filled = spaces(text, width);
    if color {
        format!("{code}{filled}\x1b[0m")
    } else {
        filled
    }
}

fn spaces(text: &str, width: usize) -> String {
    let mut out = text.to_string();
    for _ in text.chars().count()..width {
        out.push(' ');
    }
    out
}
