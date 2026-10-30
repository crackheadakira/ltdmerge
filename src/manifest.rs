use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::category::PartsEntry;

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct AddManifest {
    pub assets: Vec<PartsEntry>,
}

impl AddManifest {
    pub fn from_json(src: &str) -> Result<Self> {
        let raw: Value = serde_json::from_str(src)?;
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
                        entry.set_overridden(byml, true);
                    }
                }
            }
        }

        Ok(manifest)
    }

    pub fn to_json(&self) -> Result<String> {
        let mut assets = Vec::new();
        for entry in &self.assets {
            let Value::Object(full) = serde_json::to_value(entry)? else {
                anyhow::bail!("asset did not serialize to an object");
            };

            let mut kept = Map::new();
            for (key, value) in full {
                let keep = match PartsEntry::BYML_FIELDS
                    .iter()
                    .find(|(rust, _)| *rust == key)
                {
                    Some((_, byml)) => key == "category" || entry.is_overridden(byml),
                    None => !value.is_null(),
                };

                if keep {
                    kept.insert(key, pruned(value));
                }
            }

            assets.push(Value::Object(kept));
        }

        let mut root = Map::new();
        root.insert("assets".into(), Value::Array(assets));
        Ok(serde_json::to_string_pretty(&Value::Object(root))?)
    }
}

fn pruned(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, pruned(value)))
                .filter(|(_, value)| !value.is_null())
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(pruned).collect()),
        other => other,
    }
}
