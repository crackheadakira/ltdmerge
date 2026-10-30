use crate::category::{PartsEntry, Vector2i};
use crate::typed_param::{self, murmur32};
use crate::util::byml_root_array_mut;
use anyhow::{Context, Result, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tomolib::formats::byml::{Byml, Value};

#[derive(Clone, Copy)]
pub struct LinkedTable {
    pub component: &'static str,
    pub class: &'static str,
    pub dir: &'static str,
    pub table: &'static str,
    pub template: &'static str,
    pub overrides: fn(&PartsEntry) -> LinkedOverrides,
}

#[derive(Default)]
pub struct LinkedOverrides {
    pub template: Option<String>,
    pub fields: BTreeMap<String, Value>,
}

pub const EYE_ACCESSORY: LinkedTable = LinkedTable {
    component: "EyeAccessoryRef",
    class: "mii__EyeAccessoryParam",
    dir: "Work/Mii/EyeAccessoryParam",
    table: "RSDB/MiiEyeAccessoryParam.Product.100.rstbl.byml.zs",
    template: "Eye000",
    overrides: |entry| {
        entry
            .eye_accessory
            .as_ref()
            .map(EyeAccessoryParam::overrides)
            .unwrap_or_default()
    },
};

impl LinkedTable {
    pub fn path(&self, file_name: &str) -> String {
        format!("{}/{file_name}.{}.gyml", self.dir, self.class)
    }

    pub fn row_id(&self, file_name: &str) -> u32 {
        murmur32(&self.path(file_name))
    }

    pub fn insert(&self, doc: &mut Byml, entry: &mut PartsEntry) -> Result<()> {
        let LinkedOverrides { template, fields } = (self.overrides)(entry);
        let template = template.as_deref().unwrap_or(self.template);
        let template_id = self.row_id(template);
        let rows = byml_root_array_mut(doc)?;

        let Some(Value::Dict(mut row)) = rows
            .iter()
            .find(|row| row_id_of(row) == Some(template_id))
            .cloned()
        else {
            bail!("{}: no row for template '{template}'", self.table);
        };

        row.extend(fields);
        typed_param::validate(self.class, &row)?;

        let id = self.row_id(&entry.file_name);
        row.insert("__RowId".into(), Value::U32(id));

        match rows.binary_search_by_key(&id, |row| row_id_of(row).unwrap_or_default()) {
            Ok(_) => bail!(
                "{}: row for '{}' already exists",
                self.table,
                entry.file_name
            ),
            Err(position) => rows.insert(position, Value::Dict(row)),
        }

        entry
            .components
            .insert(self.component.into(), self.path(&entry.file_name));
        entry.sync_component_hashes();
        Ok(())
    }

    pub fn retarget(&self, entry: &mut PartsEntry, old_file_name: &str) {
        if entry.components_hash.get(self.component) != Some(&self.row_id(old_file_name)) {
            return;
        }

        entry
            .components_hash
            .insert(self.component.into(), self.row_id(&entry.file_name));

        if let Some(path) = entry.components.get_mut(self.component) {
            *path = self.path(&entry.file_name);
        }
    }
}

pub fn row_id_of(row: &Value) -> Option<u32> {
    match row {
        Value::Dict(map) => match map.get("__RowId")? {
            Value::U32(id) => Some(*id),
            Value::I32(id) => Some(*id as u32),
            _ => None,
        },
        _ => None,
    }
}

pub fn rows_by_id(doc: &Byml) -> Result<BTreeMap<u32, Value>> {
    let mut rows = BTreeMap::new();

    for row in crate::util::byml_root_array(doc)? {
        let id = row_id_of(row).context("linked row missing a u32 __RowId")?;

        anyhow::ensure!(
            rows.insert(id, row.clone()).is_none(),
            "duplicate linked row {id}"
        );
    }

    Ok(rows)
}

pub fn set_row_id(row: &mut Value, id: u32) {
    if let Value::Dict(map) = row {
        map.insert("__RowId".into(), Value::U32(id));
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct EyeAccessoryParam {
    pub template: Option<String>,

    #[serde(skip_serializing_if = "EyeAccessoryPart::is_empty")]
    pub eyelash_upper: EyeAccessoryPart,

    #[serde(skip_serializing_if = "EyeAccessoryPart::is_empty")]
    pub eyelash_lower: EyeAccessoryPart,

    #[serde(skip_serializing_if = "EyeAccessoryPart::is_empty")]
    pub eyelid_upper: EyeAccessoryPart,

    #[serde(skip_serializing_if = "EyeAccessoryPart::is_empty")]
    pub eyelid_lower: EyeAccessoryPart,

    #[serde(skip_serializing_if = "EyeAccessoryPart::is_empty")]
    pub highlight: EyeAccessoryPart,

    pub eyeline_default_enable: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct EyeAccessoryPart {
    pub default_parts_index: Option<i32>,
    pub pos: Option<Vector2i>,
    pub rotate: Option<i32>,
    pub scale: Option<i32>,
    pub aspect: Option<i32>,
}

impl EyeAccessoryPart {
    fn is_empty(&self) -> bool {
        self.default_parts_index.is_none()
            && self.pos.is_none()
            && self.rotate.is_none()
            && self.scale.is_none()
            && self.aspect.is_none()
    }
}

impl EyeAccessoryParam {
    fn overrides(&self) -> LinkedOverrides {
        let mut fields = BTreeMap::new();
        for (prefix, part) in [
            ("EyelashUpper", &self.eyelash_upper),
            ("EyelashLower", &self.eyelash_lower),
            ("EyelidUpper", &self.eyelid_upper),
            ("EyelidLower", &self.eyelid_lower),
            ("Highlight", &self.highlight),
        ] {
            let scalars = [
                ("DefaultPartsIndex", part.default_parts_index),
                ("Rotate", part.rotate),
                ("Scale", part.scale),
                ("Aspect", part.aspect),
            ];

            for (name, value) in scalars {
                if let Some(value) = value {
                    fields.insert(format!("{prefix}{name}"), Value::I32(value));
                }
            }

            if let Some(pos) = &part.pos {
                fields.insert(format!("{prefix}Pos"), pos.to_byml());
            }
        }

        if let Some(enable) = self.eyeline_default_enable {
            fields.insert("EyelineDefaultEnable".into(), Value::Bool(enable));
        }

        LinkedOverrides {
            template: self.template.clone(),
            fields,
        }
    }
}
