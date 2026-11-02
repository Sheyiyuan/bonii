use crate::model::Catalog;
use anyhow::{Context, Result};

pub fn load() -> Result<Catalog> {
    toml::from_str(crate::model::CATALOG_TOML).context("解析内置源目录失败")
}
