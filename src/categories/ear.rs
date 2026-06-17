use crate::category::{
    CategoryDef, CompiledAsset, PartsEntry, parse_pack_entry_common, parse_rstbl_entry_common,
};
use crate::impl_as_any;
use crate::params::{AssetParams, downcast_params};
use crate::util::bfres_parse;
use anyhow::{Context, Result};
use schemars::{JsonSchema, Schema, schema_for};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use tomolib::formats::byml::Value;

const VANILLA_MAX_PARTS: i32 = 3;
const FALLBACK_ICON: &str = "MiiEditor_Face_Ear00_Uit";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EarParams {
    pub model: String,

    #[serde(skip)]
    #[schemars(skip)]
    pub(crate) model_name: String,
}

impl AssetParams for EarParams {
    fn primary_source(&self) -> &str {
        &self.model
    }

    impl_as_any!(EarParams);
}

pub struct EarDef;

impl CategoryDef for EarDef {
    fn category_name(&self) -> &str {
        "Ear"
    }

    fn parts_type_hash(&self) -> u32 {
        0xBC6D99A0
    }

    fn vanilla_max_parts_index(&self) -> i32 {
        VANILLA_MAX_PARTS
    }

    fn part_name(&self, index: u32) -> String {
        format!("Ear{index:02}")
    }

    fn row_id(&self, index: u32) -> String {
        format!("Work/Mii/Parts/Ear{index:02}.mii__Parts.gyml")
    }

    fn internal_model_name(&self, index: u32) -> String {
        format!("Ear{index:02}")
    }

    fn pack_path(&self, file_name: &str) -> String {
        format!("Mii/Parts/{file_name}.mii__Parts.bgyml")
    }

    fn vanilla_icon_fallback(&self) -> &str {
        FALLBACK_ICON
    }

    fn path_parts_order(&self) -> &str {
        "Mii/PartsOrder/Ear.mii__PartsOrder.bgyml"
    }

    fn matches_icon_name(&self, tex_name: &str) -> bool {
        tex_name.contains("MiiEditor_Face_Ear")
    }

    fn editor_icon_name(&self, index: u32) -> String {
        format!("MiiEditor_Face_{}_Uit", self.part_name(index))
    }

    fn json_schema(&self) -> Schema {
        schema_for!(EarParams)
    }

    fn parse_asset_params(&self, params: &serde_json::Value) -> Result<Box<dyn AssetParams>> {
        let p: EarParams = serde_json::from_value(params.clone())
            .map_err(|e| anyhow::anyhow!("Ear params: {e}"))?;
        Ok(Box::new(p))
    }

    fn parse_rstbl_entry(&self, val: &Value) -> Result<Option<PartsEntry>> {
        parse_rstbl_entry_common(val, "Ear", VANILLA_MAX_PARTS)
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
        let p = downcast_params::<EarParams>(params, self.category_name())?;

        let file_name = self.part_name(index);
        let row_id = self.row_id(index);
        let icon = editor_icon_name.unwrap_or_else(|| self.vanilla_icon_fallback().to_string());

        let folder = self.internal_model_name(index);
        let fmdb = format!(
            "Work/Model/Mii/MiiEar/{folder}/output/{}.fmdb",
            p.model_name
        );

        let mut rstbl_raw = rstbl_template.clone();
        rstbl_raw.insert(
            "Category".into(),
            Value::String(self.category_name().into()),
        );
        rstbl_raw.insert("FileName".into(), Value::String(file_name.clone()));
        rstbl_raw.insert("PartsIndex".into(), Value::I32(index as i32));
        rstbl_raw.insert("__RowId".into(), Value::String(row_id.clone()));
        rstbl_raw.insert("EditorIconName".into(), Value::String(icon.clone()));

        if let Some(Value::Dict(mu)) = rstbl_raw.get_mut("ModelUnit") {
            mu.insert("Fmdb".into(), Value::String(fmdb.clone()));
        }

        let mut pack_raw = BTreeMap::new();
        pack_raw.insert(
            "Category".into(),
            Value::String(self.category_name().into()),
        );
        pack_raw.insert("EditorIconName".into(), Value::String(icon.clone()));
        pack_raw.insert("FileName".into(), Value::String(file_name.clone()));
        pack_raw.insert("IsVisibleInEditor".into(), Value::Bool(true));
        pack_raw.insert("PartsIndex".into(), Value::I32(index as i32));
        pack_raw.insert("ModelUnit".into(), {
            let mut mu = BTreeMap::new();
            mu.insert("Fmdb".into(), Value::String(fmdb.clone()));
            Value::Dict(mu)
        });

        Ok(PartsEntry {
            parts_index: index as i32,
            file_name,
            row_id,
            editor_icon_name: Some(icon),
            rstbl_raw,
            pack_raw,
        })
    }

    fn compile_asset(
        &self,
        _index: u32,
        target_token: &str,
        params: &mut dyn AssetParams,
    ) -> Result<CompiledAsset> {
        let p = crate::params::downcast_params_mut::<EarParams>(params, self.category_name())?;

        let model_bytes = crate::util::read_and_decompress(Path::new(""), &p.model)?;
        let mut model_bfres = bfres_parse(&model_bytes)?;

        p.model_name = model_bfres
            .models
            .names
            .get(0)
            .context("The provided BFRES file contains no internal meshes.")?
            .clone();

        model_bfres.name = target_token.to_string();
        let serialized = model_bfres.write().context("BFRES serialization failed")?;

        let mut romfs_files = BTreeMap::new();
        let target_bfres_filename = format!("{target_token}.bfres");
        romfs_files.insert(format!("Model/{target_bfres_filename}"), serialized);

        Ok(CompiledAsset {
            pack_files: BTreeMap::new(),
            romfs_files,
        })
    }
}
