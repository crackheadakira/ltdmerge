use crate::category::{CategoryDef, PartsEntry, parse_pack_entry_common, parse_rstbl_entry_common};
use crate::impl_as_any;
use crate::params::{AssetParams, downcast_params};
use anyhow::Result;
use schemars::{JsonSchema, Schema, schema_for};
use serde::Deserialize;
use std::collections::BTreeMap;
use tomolib::formats::byml::Value;

const VANILLA_MAX_PARTS: i32 = 62;
const FALLBACK_ICON: &str = "MiiEditor_Face_Eye001_Uit";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EyeParams {
    pub texture: String,
}

impl AssetParams for EyeParams {
    fn primary_source(&self) -> &str {
        &self.texture
    }

    impl_as_any!(EyeParams);
}

pub struct EyeDef;

impl CategoryDef for EyeDef {
    fn category_name(&self) -> &str {
        "Eye"
    }

    fn parts_type_hash(&self) -> u32 {
        2803465551
    }

    fn vanilla_max_parts_index(&self) -> i32 {
        VANILLA_MAX_PARTS
    }

    fn part_name(&self, index: u32) -> String {
        format!("Eye{:03}", index)
    }

    fn row_id(&self, index: u32) -> String {
        format!("Work/Mii/Parts/Eye{:03}.mii__Parts.gyml", index)
    }

    fn internal_model_name(&self, _index: u32) -> String {
        String::new()
    }

    fn pack_path(&self, file_name: &str) -> String {
        format!("Mii/Parts/{file_name}.mii__Parts.bgyml")
    }

    fn vanilla_icon_fallback(&self) -> &str {
        FALLBACK_ICON
    }

    fn path_parts_order(&self) -> &str {
        "Mii/PartsOrder/Eye.mii__PartsOrder.bgyml"
    }

    fn matches_texture_name(&self, name: &str) -> bool {
        name.starts_with("Eye")
    }

    fn matches_icon_name(&self, tex_name: &str) -> bool {
        tex_name.contains("MiiEditor_Face_Eye")
    }

    fn editor_icon_name(&self, index: u32) -> String {
        format!("MiiEditor_Face_{}_Uit", self.part_name(index))
    }

    fn extra_index_fields(&self, index: u32) -> Vec<(&'static str, String)> {
        vec![
            ("TextureName", self.part_name(index)),
            (
                "EditorMaskIconName",
                format!("MiiEditor_Face_{}color_Uit", self.part_name(index)),
            ),
        ]
    }

    fn extra_remappable_string_keys(&self) -> &[&'static str] {
        &["TextureName"]
    }

    fn parse_asset_params(&self, params: &serde_json::Value) -> Result<Box<dyn AssetParams>> {
        let p: EyeParams = serde_json::from_value(params.clone())
            .map_err(|e| anyhow::anyhow!("Eye params: {e}"))?;
        Ok(Box::new(p))
    }

    fn parse_rstbl_entry(&self, val: &Value) -> Result<Option<PartsEntry>> {
        parse_rstbl_entry_common(val, "Eye", VANILLA_MAX_PARTS)
    }

    fn parse_pack_entry(&self, map: &BTreeMap<String, Value>) -> Result<Option<PartsEntry>> {
        parse_pack_entry_common(map, VANILLA_MAX_PARTS)
    }

    fn new_entry(
        &self,
        index: u32,
        editor_icon_name: Option<String>,
        params: &dyn AssetParams,
        rstbl_template: &BTreeMap<String, Value>,
    ) -> Result<PartsEntry> {
        let _ = downcast_params::<EyeParams>(params, self.category_name())?;

        let file_name = self.part_name(index);
        let row_id = self.row_id(index);
        let icon = editor_icon_name.unwrap_or_else(|| self.vanilla_icon_fallback().to_string());
        let texture_name = self.part_name(index);
        let mask_icon = format!("MiiEditor_Face_{}color_Uit", file_name);

        let mut rstbl_raw = rstbl_template.clone();
        rstbl_raw.insert(
            "Category".into(),
            Value::String(self.category_name().into()),
        );
        rstbl_raw.insert("FileName".into(), Value::String(file_name.clone()));
        rstbl_raw.insert("PartsIndex".into(), Value::I32(index as i32));
        rstbl_raw.insert("__RowId".into(), Value::String(row_id.clone()));
        rstbl_raw.insert("EditorIconName".into(), Value::String(icon.clone()));
        rstbl_raw.insert("TextureName".into(), Value::String(texture_name.clone()));
        rstbl_raw.insert(
            "EditorMaskIconName".into(),
            Value::String(mask_icon.clone()),
        );

        let mut pack_raw = BTreeMap::new();
        pack_raw.insert("AxisForExpression".into(), {
            rstbl_template
                .get("AxisForExpression")
                .cloned()
                .unwrap_or_else(|| {
                    let mut m = BTreeMap::new();
                    m.insert("X".into(), Value::F32(0.0));
                    m.insert("Y".into(), Value::F32(0.0));
                    Value::Dict(m)
                })
        });

        pack_raw.insert(
            "Category".into(),
            Value::String(self.category_name().into()),
        );

        let mut components = BTreeMap::new();
        components.insert(
            "EyeAccessoryRef".into(),
            Value::String(format!(
                "Work/Mii/EyeAccessoryParam/{}.mii__EyeAccessoryParam.gyml",
                file_name
            )),
        );
        pack_raw.insert("Components".into(), Value::Dict(components));

        if let Some(ch) = rstbl_template.get("ComponentsHash") {
            pack_raw.insert("ComponentsHash".into(), ch.clone());
        }

        pack_raw.insert("EditorIconName".into(), Value::String(icon.clone()));
        pack_raw.insert("EditorMaskIconName".into(), Value::String(mask_icon));
        pack_raw.insert("FileName".into(), Value::String(file_name.clone()));
        pack_raw.insert("IsVisibleInEditor".into(), Value::Bool(true));
        pack_raw.insert("PartsIndex".into(), Value::I32(index as i32));

        if let Some(sz) = rstbl_template.get("SizeForExpression") {
            pack_raw.insert("SizeForExpression".into(), sz.clone());
        }

        pack_raw.insert("TextureName".into(), Value::String(texture_name));

        Ok(PartsEntry {
            parts_index: index as i32,
            file_name,
            row_id,
            editor_icon_name: Some(icon),
            rstbl_raw,
            pack_raw,
        })
    }

    fn json_schema(&self) -> Schema {
        schema_for!(EyeParams)
    }
}
