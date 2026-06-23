use crate::category::{CategoryDef, CompiledAsset, PartsEntry};
use crate::impl_as_any;
use crate::params::{AssetParams, downcast_params};
use crate::util::bfres_parse;
use anyhow::{Context, Result};
use schemars::{JsonSchema, Schema, schema_for};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use tomolib::formats::byml::Value;

const VANILLA_MAX_PARTS: i32 = 21;
const FALLBACK_ICON: &str = "MiiEditor_Face_Faceline15_Uit";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FacelineParams {
    pub model: String,

    #[serde(skip)]
    #[schemars(skip)]
    pub(crate) model_name: String,
}

impl AssetParams for FacelineParams {
    fn primary_source(&self) -> &str {
        &self.model
    }

    impl_as_any!(FacelineParams);
}

pub struct FacelineDef;

impl CategoryDef for FacelineDef {
    fn category_name(&self) -> &str {
        "Faceline"
    }

    fn parts_type_hash(&self) -> u32 {
        0x2E822724
    }

    fn vanilla_max_parts_index(&self) -> i32 {
        VANILLA_MAX_PARTS
    }

    fn part_name(&self, index: u32) -> String {
        format!("Faceline{index}")
    }

    fn row_id(&self, index: u32) -> String {
        format!("Work/Mii/Parts/Faceline{index}.mii__Parts.gyml")
    }

    fn internal_model_name(&self, index: u32) -> String {
        format!("MiiHead{index:02}")
    }

    fn pack_path(&self, file_name: &str) -> String {
        format!("Mii/Parts/{file_name}.mii__Parts.bgyml")
    }

    fn vanilla_icon_fallback(&self) -> &str {
        FALLBACK_ICON
    }

    fn path_parts_order(&self) -> &str {
        "Mii/PartsOrder/Faceline.mii__PartsOrder.bgyml"
    }

    fn matches_icon_name(&self, tex_name: &str) -> bool {
        tex_name.contains("MiiEditor_Face_Faceline")
    }

    fn editor_icon_name(&self, index: u32) -> String {
        format!("MiiEditor_Face_{}_Uit", self.part_name(index))
    }

    fn editor_mask_icon_name(&self, index: u32) -> String {
        format!("MiiEditor_Face_{}color_Uit", self.part_name(index))
    }

    fn json_schema(&self) -> Schema {
        schema_for!(FacelineParams)
    }

    fn parse_asset_params(&self, params: &serde_json::Value) -> Result<Box<dyn AssetParams>> {
        let p: FacelineParams = serde_json::from_value(params.clone())
            .map_err(|e| anyhow::anyhow!("Faceline params: {e}"))?;
        Ok(Box::new(p))
    }

    fn apply_category_defaults(&self, index: u32, mut entry: PartsEntry) -> PartsEntry {
        entry.parts_index = index as i32;
        entry.parts_type = self.parts_type_hash();
        entry.category = self.category_name().to_string();
        entry.file_name = self.part_name(index);
        entry.row_id = self.row_id(index);

        if entry.editor_icon_name.is_none() {
            entry.editor_icon_name = Some(self.editor_icon_name(index));
        }

        entry.max_trans_x = 18;
        entry.min_trans_y = 0;
        entry.max_trans_y = 31;
        entry.is_flippable = false;
        entry.is_use_hair_parts_model = false;

        if entry.default_scale == 0 {
            entry.default_scale = 2;
        }

        entry
    }

    fn compile_asset(
        &self,
        _index: u32,
        target_token: &str,
        params: &mut dyn AssetParams,
    ) -> Result<CompiledAsset> {
        let p = crate::params::downcast_params_mut::<FacelineParams>(params, self.category_name())?;

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
