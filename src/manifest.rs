use anyhow::Result;
use serde::Deserialize;

use crate::category::PartsEntry;

#[derive(Debug, Deserialize)]
pub struct AddManifest {
    pub assets: Vec<PartsEntry>,
}

impl AddManifest {
    pub fn from_json(src: &str) -> Result<Self> {
        let raw: serde_json::Value = serde_json::from_str(src)?;
        let mut manifest: Self = serde_json::from_value(raw.clone())
            .map_err(|e| anyhow::anyhow!("manifest JSON parse error: {e}"))?;

        for (entry, raw) in manifest
            .assets
            .iter_mut()
            .zip(raw["assets"].as_array().into_iter().flatten())
        {
            if let Some(fields) = raw.as_object() {
                for (rust, byml) in PartsEntry::BYML_FIELDS {
                    if fields.contains_key(*rust) {
                        entry.origin.manifest_fields.insert((*byml).into());
                    }
                }
            }
        }

        Ok(manifest)
    }
}
