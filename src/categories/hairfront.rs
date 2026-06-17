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
use tomolib::formats::byml::Value;

const VANILLA_MAX_PARTS: i32 = 82;
const FALLBACK_ICON: &str = "MiiEditor_Face_HairFront001_Uit";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HairFrontParams {
    pub model: String,
    #[serde(skip)]
    #[schemars(skip)]
    pub(crate) model_name: String,

    pub hat_model: Option<String>,
    #[serde(skip)]
    #[schemars(skip)]
    pub(crate) hat_model_name: String,

    pub phcl: Option<String>,
    pub hat_phcl: Option<String>,
}

impl HairFrontParams {
    pub fn hat_model_resolved(&self) -> &str {
        self.hat_model.as_deref().unwrap_or(&self.model)
    }

    pub fn phcl_resolved(&self) -> &str {
        self.phcl.as_deref().unwrap_or("")
    }

    pub fn hat_phcl_resolved(&self) -> &str {
        self.hat_phcl.as_deref().unwrap_or("")
    }
}

impl AssetParams for HairFrontParams {
    fn primary_source(&self) -> &str {
        &self.model
    }

    impl_as_any!(HairFrontParams);
}

pub struct HairFrontDef;

impl CategoryDef for HairFrontDef {
    fn category_name(&self) -> &str {
        "HairFront"
    }

    fn parts_type_hash(&self) -> u32 {
        3521434781
    }

    fn vanilla_max_parts_index(&self) -> i32 {
        VANILLA_MAX_PARTS
    }

    fn part_name(&self, index: u32) -> String {
        format!("HairFront{:03}", index)
    }

    fn row_id(&self, index: u32) -> String {
        format!("Work/Mii/Parts/HairFront{:03}.mii__Parts.gyml", index)
    }

    fn internal_model_name(&self, index: u32) -> String {
        format!("MiiHairFront{:03}", index)
    }

    fn pack_path(&self, file_name: &str) -> String {
        format!("Mii/Parts/{file_name}.mii__Parts.bgyml")
    }

    fn vanilla_icon_fallback(&self) -> &str {
        FALLBACK_ICON
    }

    fn path_parts_order(&self) -> &str {
        "Mii/PartsOrder/HairFront.mii__PartsOrder.bgyml"
    }

    fn matches_texture_name(&self, name: &str) -> bool {
        name.starts_with("MiiParts_HairFront")
    }

    fn matches_icon_name(&self, tex_name: &str) -> bool {
        tex_name.contains("MiiEditor_Face_HairFront")
    }

    fn editor_icon_name(&self, index: u32) -> String {
        format!("MiiEditor_Face_{:03}_Uit", self.part_name(index))
    }

    fn parse_asset_params(&self, params: &serde_json::Value) -> Result<Box<dyn AssetParams>> {
        let p: HairFrontParams = serde_json::from_value(params.clone())
            .map_err(|e| anyhow::anyhow!("HairFront params: {e}"))?;
        Ok(Box::new(p))
    }

    fn parse_rstbl_entry(&self, val: &Value) -> Result<Option<PartsEntry>> {
        parse_rstbl_entry_common(val, "HairFront", VANILLA_MAX_PARTS)
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
        let p = downcast_params::<HairFrontParams>(params, self.category_name())?;

        let file_name = self.part_name(index);
        let row_id = self.row_id(index);
        let icon = editor_icon_name.unwrap_or_else(|| self.vanilla_icon_fallback().to_string());

        let template_index = match rstbl_template.get("PartsIndex") {
            Some(Value::I32(n)) => *n as u32,
            Some(Value::U32(n)) => *n,
            _ => 1,
        };

        let main_fmdb = format!(
            "Work/Model/Mii/MiiHairFront/MiiHairFront{template_index:03}/output/{}.fmdb",
            p.model_name,
        );

        let hat_model_name = if p.hat_model.is_some() {
            &p.hat_model_name
        } else {
            &p.model_name
        };

        let hat_fmdb = format!(
            "Work/Model/Mii/MiiHairFront/MiiHairFront{template_index:03}/output/{hat_model_name}.fmdb",
        );

        let mut rstbl_raw = rstbl_template.clone();
        rstbl_raw.insert(
            "Category".into(),
            Value::String(self.category_name().into()),
        );

        rstbl_raw.insert("FileName".into(), Value::String(file_name.clone()));
        rstbl_raw.insert("PartsIndex".into(), Value::I32(index as i32));
        rstbl_raw.insert("EditorIconName".into(), Value::String(icon.clone()));
        rstbl_raw.insert("IsVisibleInEditor".into(), Value::Bool(true));
        rstbl_raw.insert("__RowId".into(), Value::String(row_id.clone()));

        for map in [&mut rstbl_raw] {
            insert_model_unit(map, &main_fmdb, p.phcl_resolved());
            insert_model_unit_for_hat(map, &hat_fmdb, p.hat_phcl_resolved());
        }

        let mut pack_raw = BTreeMap::new();

        for key in [
            "BabyHairNameFatherFemale",
            "BabyHairNameFatherMale",
            "BabyHairNameMotherFemale",
            "BabyHairNameMotherMale",
            "IsSelectableSubColor",
        ] {
            if let Some(val) = rstbl_template.get(key) {
                pack_raw.insert(key.into(), val.clone());
            }
        }

        pack_raw.insert(
            "Category".into(),
            Value::String(self.category_name().into()),
        );
        pack_raw.insert("Components".into(), Value::Dict(BTreeMap::new()));
        pack_raw.insert("ComponentsHash".into(), Value::Dict(BTreeMap::new()));
        pack_raw.insert("EditorIconName".into(), Value::String(icon.clone()));
        pack_raw.insert("FileName".into(), Value::String(file_name.clone()));
        pack_raw.insert("IsVisibleInEditor".into(), Value::Bool(true));
        pack_raw.insert("PartsIndex".into(), Value::I32(index as i32));
        insert_model_unit(&mut pack_raw, &main_fmdb, p.phcl_resolved());
        insert_model_unit_for_hat(&mut pack_raw, &hat_fmdb, p.hat_phcl_resolved());

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
        let p =
            crate::params::downcast_params_mut::<HairFrontParams>(params, self.category_name())?;
        let mut romfs_files = BTreeMap::new();

        let main_bytes = crate::util::read_and_decompress(std::path::Path::new(""), &p.model)?;
        let mut main_bfres = bfres_parse(&main_bytes)?;
        main_bfres.name = target_token.to_string();
        p.model_name = main_bfres
            .models
            .names
            .get(0)
            .context("The provided BFRES file contains no internal meshes.")?
            .clone();

        romfs_files.insert(format!("Model/{target_token}.bfres"), main_bfres.write()?);

        if let Some(ref hat_source) = p.hat_model {
            let hat_bytes = crate::util::read_and_decompress(std::path::Path::new(""), hat_source)?;
            let mut hat_bfres = bfres_parse(&hat_bytes)?;
            hat_bfres.name = format!("{target_token}_Hat");

            p.hat_model_name = hat_bfres
                .models
                .names
                .get(0)
                .context("The provided BFRES file contains no internal meshes.")?
                .clone();

            romfs_files.insert(
                format!("Model/{target_token}_Hat.bfres"),
                hat_bfres.write()?,
            );
        }

        if let Some(ref phcl_path) = p.phcl {
            let raw_phcl = std::fs::read(phcl_path)?;
            romfs_files.insert(format!("Phive/Cloth/{target_token}.phcl"), raw_phcl);
        }

        Ok(CompiledAsset {
            pack_files: BTreeMap::new(),
            romfs_files,
        })
    }

    fn json_schema(&self) -> Schema {
        schema_for!(HairFrontParams)
    }
}

fn insert_model_unit(map: &mut BTreeMap<String, Value>, fmdb: &str, phcl: &str) {
    let mut mu = BTreeMap::new();
    mu.insert("Fmdb".into(), Value::String(fmdb.to_string()));
    mu.insert("PhivePhcl".into(), Value::String(phcl.to_string()));
    map.insert("ModelUnit".into(), Value::Dict(mu));
}

fn insert_model_unit_for_hat(map: &mut BTreeMap<String, Value>, fmdb: &str, phcl: &str) {
    let mut mu = BTreeMap::new();
    mu.insert("Fmdb".into(), Value::String(fmdb.to_string()));
    mu.insert("PhivePhcl".into(), Value::String(phcl.to_string()));
    map.insert("ModelUnitForHat".into(), Value::Dict(mu));
}
