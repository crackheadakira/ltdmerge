use crate::category::{
    CategoryDef, CompiledAsset, PartsEntry, parse_pack_entry_common, parse_rstbl_entry_common,
};
use crate::impl_as_any;
use crate::params::{AssetParams, downcast_params, downcast_params_mut};
use crate::util::{bfres_parse, read_and_decompress};
use anyhow::{Context, Result};
use schemars::{JsonSchema, Schema, schema_for};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use tomolib::formats::byml::Value;

const VANILLA_MAX_PARTS: i32 = 337;
const FALLBACK_ICON: &str = "MiiEditor_Face_HairBack001_Uit";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HairBackParams {
    pub model: String,
    pub hat_model: Option<String>,
    pub phcl: Option<String>,
    pub hat_phcl: Option<String>,

    #[serde(skip)]
    #[schemars(skip)]
    pub(crate) model_name: String,

    #[serde(skip)]
    #[schemars(skip)]
    pub(crate) hat_model_name: String,
}

impl HairBackParams {
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

impl AssetParams for HairBackParams {
    fn primary_source(&self) -> &str {
        &self.model
    }

    impl_as_any!(HairBackParams);
}

pub struct HairBackDef;

impl CategoryDef for HairBackDef {
    fn category_name(&self) -> &str {
        "HairBack"
    }

    fn internal_category_name(&self) -> &str {
        "Hair"
    }

    fn parts_type_hash(&self) -> u32 {
        0xDD07FC3F
    }

    fn vanilla_max_parts_index(&self) -> i32 {
        VANILLA_MAX_PARTS
    }

    fn part_name(&self, index: u32) -> String {
        format!("HairBack{:03}", index)
    }

    fn row_id(&self, index: u32) -> String {
        format!("Work/Mii/Parts/HairBack{:03}.mii__Parts.gyml", index)
    }

    fn internal_model_name(&self, index: u32) -> String {
        format!("MiiHairBack{:03}", index)
    }

    fn pack_path(&self, file_name: &str) -> String {
        format!("Mii/Parts/{file_name}.mii__Parts.bgyml")
    }

    fn vanilla_icon_fallback(&self) -> &str {
        FALLBACK_ICON
    }

    fn path_parts_order(&self) -> &str {
        "Mii/PartsOrder/HairBack.mii__PartsOrder.bgyml"
    }

    fn matches_texture_name(&self, name: &str) -> bool {
        name.starts_with("MiiParts_HairBack")
    }

    fn matches_icon_name(&self, tex_name: &str) -> bool {
        tex_name.contains("MiiEditor_Face_HairBack")
    }

    fn editor_icon_name(&self, index: u32) -> String {
        format!("MiiEditor_Face_{}_Uit", self.part_name(index))
    }

    fn parse_asset_params(&self, params: &serde_json::Value) -> Result<Box<dyn AssetParams>> {
        let p: HairBackParams = serde_json::from_value(params.clone())
            .map_err(|e| anyhow::anyhow!("HairBack params: {e}"))?;
        Ok(Box::new(p))
    }

    fn parse_rstbl_entry(&self, val: &Value) -> Result<Option<PartsEntry>> {
        parse_rstbl_entry_common(val, "Hair", VANILLA_MAX_PARTS)
    }

    fn parse_pack_entry(&self, map: &BTreeMap<String, Value>) -> Result<Option<PartsEntry>> {
        parse_pack_entry_common(map, VANILLA_MAX_PARTS)
    }

    fn compile_asset(
        &self,
        _index: u32,
        target_token: &str,
        params: &mut dyn AssetParams,
    ) -> Result<CompiledAsset> {
        let p = downcast_params_mut::<HairBackParams>(params, self.category_name())?;
        let mut romfs_files = BTreeMap::new();

        let main_bytes = read_and_decompress(Path::new(""), &p.model)?;
        let mut main_bfres = bfres_parse(&main_bytes)?;
        p.model_name = main_bfres
            .models
            .names
            .get(0)
            .context("main bfres contains no internal meshes")?
            .clone();
        main_bfres.name = target_token.to_string();
        romfs_files.insert(format!("Model/{target_token}.bfres"), main_bfres.write()?);

        let hat_token = format!("{target_token}Hat");
        if let Some(ref hat_source) = p.hat_model.clone() {
            let hat_bytes = read_and_decompress(Path::new(""), hat_source)?;
            let mut hat_bfres = bfres_parse(&hat_bytes)?;
            p.hat_model_name = hat_bfres
                .models
                .names
                .get(0)
                .context("hat bfres contains no internal meshes")?
                .clone();
            hat_bfres.name = hat_token.clone();
            romfs_files.insert(format!("Model/{hat_token}.bfres"), hat_bfres.write()?);
        } else {
            p.hat_model_name = p.model_name.clone();
        }

        if let Some(ref phcl_path) = p.phcl {
            let raw =
                std::fs::read(phcl_path).with_context(|| format!("reading phcl '{phcl_path}'"))?;
            romfs_files.insert(format!("Phive/Cloth/{target_token}.phcl"), raw);
        }

        if let Some(ref hat_phcl_path) = p.hat_phcl {
            let raw = std::fs::read(hat_phcl_path)
                .with_context(|| format!("reading hat phcl '{hat_phcl_path}'"))?;
            romfs_files.insert(format!("Phive/Cloth/{hat_token}.phcl"), raw);
        }

        Ok(CompiledAsset {
            pack_files: BTreeMap::new(),
            romfs_files,
        })
    }

    fn new_entry(
        &self,
        index: u32,
        editor_icon_name: Option<String>,
        params: &dyn AssetParams,
        rstbl_template: &BTreeMap<String, Value>,
    ) -> Result<PartsEntry> {
        let p = downcast_params::<HairBackParams>(params, self.category_name())?;

        let file_name = self.part_name(index);
        let row_id = self.row_id(index);
        let icon = editor_icon_name.unwrap_or_else(|| self.vanilla_icon_fallback().to_string());

        let template_index = match rstbl_template.get("PartsIndex") {
            Some(Value::I32(n)) => *n as u32,
            Some(Value::U32(n)) => *n,
            _ => 1,
        };

        let token = self.internal_model_name(template_index);

        let phcl = if p.phcl.is_some() {
            format!("Work/Phive/Cloth/{token}.phcl")
        } else {
            String::new()
        };

        let main_fmdb = format!(
            "Work/Model/Mii/MiiHairBack/{token}/output/{}.fmdb",
            p.model_name,
        );
        let hat_fmdb = format!(
            "Work/Model/Mii/MiiHairBack/{token}/output/{}.fmdb",
            p.hat_model_name,
        );

        let mut rstbl_raw = rstbl_template.clone();
        rstbl_raw.insert("Category".into(), Value::String("Hair".into()));
        rstbl_raw.insert("FileName".into(), Value::String(file_name.clone()));
        rstbl_raw.insert("PartsIndex".into(), Value::I32(index as i32));
        rstbl_raw.insert("EditorIconName".into(), Value::String(icon.clone()));
        rstbl_raw.insert("IsVisibleInEditor".into(), Value::Bool(true));
        rstbl_raw.insert("__RowId".into(), Value::String(row_id.clone()));
        insert_model_unit(&mut rstbl_raw, &main_fmdb, &phcl);
        insert_model_unit_for_hat(&mut rstbl_raw, &hat_fmdb, &phcl);

        let mut pack_raw = BTreeMap::new();

        for key in [
            "BabyHairNameFatherFemale",
            "BabyHairNameFatherMale",
            "BabyHairNameMotherFemale",
            "BabyHairNameMotherMale",
        ] {
            if let Some(val) = rstbl_template.get(key) {
                pack_raw.insert(key.into(), val.clone());
            }
        }

        pack_raw.insert("Category".into(), Value::String("Hair".into()));
        pack_raw.insert("Components".into(), Value::Dict(BTreeMap::new()));
        pack_raw.insert("ComponentsHash".into(), Value::Dict(BTreeMap::new()));
        pack_raw.insert("EditorIconName".into(), Value::String(icon.clone()));
        pack_raw.insert("FileName".into(), Value::String(file_name.clone()));

        for key in [
            "IsAttachableHairFront",
            "IsAttachableHairPartsLower",
            "IsAttachableHairPartsMiddle",
            "IsAttachableHairPartsUpper",
        ] {
            if let Some(val) = rstbl_template.get(key) {
                pack_raw.insert(key.into(), val.clone());
            }
        }

        pack_raw.insert("IsSelectableSubColor".into(), {
            rstbl_template
                .get("IsSelectableSubColor")
                .cloned()
                .unwrap_or(Value::Bool(false))
        });
        pack_raw.insert("IsUseHairPartsModel".into(), Value::Bool(false));
        pack_raw.insert("IsVisibleInEditor".into(), Value::Bool(true));

        pack_raw.insert("HairPartsAttachInfo".into(), Value::Array(vec![]));

        insert_model_unit(&mut pack_raw, &main_fmdb, &phcl);
        insert_model_unit_for_hat(&mut pack_raw, &hat_fmdb, &phcl);

        pack_raw.insert("PartsIndex".into(), Value::I32(index as i32));

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
        schema_for!(HairBackParams)
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
