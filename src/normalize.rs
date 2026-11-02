//! Per-ecosystem name normalisation, applied before the availability query.

pub fn apply(rule: Option<&str>, name: &str) -> String {
    match rule {
        Some("pep503") => pep503(name),
        Some("crates") => crates(name),
        Some("npm") => npm(name),
        Some("lower") => name.to_lowercase(),
        _ => name.to_string(),
    }
}

/// PyPI folds runs of `[-_.]+` into one `-` and lowercases; `foo.bar` and
/// `foo-bar` are the same package there.
pub fn pep503(name: &str) -> String {
    let lowered = name.to_lowercase();
    let mut out = String::with_capacity(lowered.len());
    let mut pending = false;
    for c in lowered.chars() {
        if c == '-' || c == '_' || c == '.' {
            pending = true;
            continue;
        }
        if pending {
            out.push('-');
            pending = false;
        }
        out.push(c);
    }
    if pending {
        out.push('-');
    }
    out
}

/// crates.io treats `-` and `_` as the same character.
pub fn crates(name: &str) -> String {
    name.to_lowercase().replace('_', "-")
}

/// npm lowercases unscoped names but keeps scoped names case-sensitive-ish;
/// the registry canonicalises the scope.
pub fn npm(name: &str) -> String {
    match name.split_once('/') {
        Some((scope, pkg)) => format!("{}/{}", scope.to_lowercase(), pkg),
        None => name.to_lowercase(),
    }
}

/// True when the input already looks like a pathed identifier (Go module path,
/// Packagist `vendor/package`) rather than a bare name.
pub fn is_pathed(name: &str) -> bool {
    name.contains('/')
}
