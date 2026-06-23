use anyhow::Result;
use serde::Deserialize;

use crate::category::PartsEntry;

#[derive(Debug, Deserialize)]
pub struct AddManifest {
    pub assets: Vec<PartsEntry>,
}

impl AddManifest {
    pub fn from_json(src: &str) -> Result<Self> {
        serde_json::from_str(src).map_err(|e| anyhow::anyhow!("manifest JSON parse error: {e}"))
    }
}
