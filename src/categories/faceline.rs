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

    fn json_schema(&self) -> Schema {
        schema_for!(FacelineParams)
    }

    fn parse_asset_params(&self, params: &serde_json::Value) -> Result<Box<dyn AssetParams>> {
        let p: FacelineParams = serde_json::from_value(params.clone())
            .map_err(|e| anyhow::anyhow!("Faceline params: {e}"))?;
        Ok(Box::new(p))
    }

    fn parse_rstbl_entry(&self, val: &Value) -> Result<Option<PartsEntry>> {
        parse_rstbl_entry_common(val, "Faceline", VANILLA_MAX_PARTS)
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
        let p = downcast_params::<FacelineParams>(params, self.category_name())?;

        let file_name = self.part_name(index);
        let row_id = self.row_id(index);
        let icon = editor_icon_name.unwrap_or_else(|| self.vanilla_icon_fallback().to_string());

        let folder = self.internal_model_name(index);
        let fmdb = format!(
            "Work/Model/Mii/MiiHead/{folder}/output/{}.fmdb",
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
            model_paths: vec![fmdb],
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::CategoryDef;

    fn make_entry(index: u32) -> PartsEntry {
        let cat = FacelineDef;
        let params = FacelineParams {
            model: format!(
                "Work/Model/Mii/MiiHead/MiiHead{index:02}/output/MiiHead{index:02}.fmdb"
            ),
            model_name: format!("MiiHead{index:02}"),
        };
        cat.new_entry(
            index,
            Some(format!("MiiEditor_Face_Faceline{index}_Uit")),
            &params,
            &BTreeMap::new(),
        )
        .unwrap()
    }

    #[test]
    fn naming() {
        let cat = FacelineDef;
        assert_eq!(cat.part_name(22), "Faceline22");
        assert_eq!(cat.internal_model_name(22), "MiiHead22");
        assert_eq!(cat.row_id(22), "Work/Mii/Parts/Faceline22.mii__Parts.gyml");
        assert_eq!(
            cat.pack_path("Faceline22"),
            "Mii/Parts/Faceline22.mii__Parts.bgyml"
        );
    }

    #[test]
    fn remap_updates_all_fields() {
        let cat = FacelineDef;
        let mut entry = make_entry(22);

        entry.pack_raw.insert(
            "DebugModelPath".into(),
            Value::String("Work/Model/Mii/MiiHead/MiiHead22/MiiHead22.fmdb".into()),
        );

        let old_token = cat.internal_model_name(22);
        let new_token = cat.internal_model_name(30);

        entry.remap_to(
            30,
            cat.part_name(30),
            cat.row_id(30),
            &old_token,
            &new_token,
            &cat,
        );

        assert_eq!(entry.parts_index, 30);
        assert_eq!(entry.file_name, "Faceline30");
        assert_eq!(entry.row_id, "Work/Mii/Parts/Faceline30.mii__Parts.gyml");
        assert!(entry.model_paths.iter().all(|p| p.contains("MiiHead30")));
        assert_eq!(
            entry.editor_icon_name.as_deref().unwrap(),
            "MiiEditor_Face_Faceline30_Uit"
        );

        match entry.pack_raw.get("DebugModelPath") {
            Some(Value::String(s)) => assert!(s.contains("MiiHead30")),
            _ => panic!("pack_raw structural replacement failed"),
        }

        match entry.pack_raw.get("PartsIndex") {
            Some(Value::I32(n)) => assert_eq!(*n, 30),
            _ => panic!("pack_raw PartsIndex not updated"),
        }
    }
}
