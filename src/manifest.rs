use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct AddManifest {
    pub assets: Vec<AssetSpec>,
}

#[derive(Debug, Deserialize)]
pub struct AssetSpec {
    pub category: String,
    pub icon: Option<PathBuf>,
    pub order_index: Option<usize>,
    pub params: serde_json::Value,
}

impl AddManifest {
    pub fn from_json(src: &str) -> Result<Self> {
        serde_json::from_str(src).map_err(|e| anyhow::anyhow!("manifest JSON parse error: {e}"))
    }
}
