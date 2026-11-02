use serde::Deserialize;

pub const CATALOG_TOML: &str = include_str!("catalog.toml");

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub user_agent: UserAgent,
    pub source: Vec<Source>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserAgent {
    pub string: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Http,
    Github,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    #[default]
    Bare,
    Pathed,
    Both,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub kind: Option<Kind>,
    #[serde(default)]
    pub query: Option<String>,
    /// `pep503` | `crates` | `npm` | `lower` | `none`
    #[serde(default)]
    pub normalize: Option<String>,
    /// percent-encode the substituted name (npm scoped packages)
    #[serde(default)]
    pub encode: bool,
    /// accepted input shape: `bare` (a plain name), `pathed` (vendor/name or
    /// host/owner/name) or `both` (npm accepts either)
    #[serde(default)]
    pub shape: Shape,
    /// pathed source additionally needs a host-like first segment (Go modules)
    #[serde(default)]
    pub require_host: bool,
    /// dotted field path to inspect; `root` or omitted inspects the whole body
    #[serde(default)]
    pub extract: Option<String>,
    #[serde(deserialize_with = "crate::util::deserialize_hit")]
    pub hit: Hit,
    #[serde(default)]
    pub info: Vec<String>,
    /// ecosystem-specific naming suffixes probed in addition to the bare name
    #[serde(default)]
    pub variants: Vec<String>,
    #[serde(default)]
    pub fuzzy: Option<Fuzzy>,
}

impl Source {
    pub fn kind(&self) -> Kind {
        self.kind.unwrap_or(Kind::Http)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum Hit {
    /// value at `extract` must be non-empty (array/object/string) or a number > 0
    Nonempty,
    /// value at `extract` must exceed the given number
    Greater(i64),
    /// HTTP 2xx alone decides, body is not parsed (plain-text endpoints)
    StatusOk,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fuzzy {
    pub endpoint: String,
    #[serde(default)]
    pub extract: Option<String>,
    /// source id whose exact query confirms a suggestion really exists
    pub verify: String,
    /// label suffix shown on fuzzy rows; defaults to `近似`
    #[serde(default)]
    pub label: Option<String>,
    /// note prepended to each fuzzy hit's detail
    #[serde(default)]
    pub note: Option<String>,
}
