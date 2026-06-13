use crate::category::{CategoryDef, PartsEntry, parse_pack_entry_common, parse_rstbl_entry_common};
use anyhow::Result;
use std::collections::BTreeMap;
use tomolib::formats::byml::Value;

const VANILLA_MAX_PARTS: i32 = 82;
const FALLBACK_ICON: &str = "MiiEditor_Face_HairFront001_Uit";

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

    fn parse_rstbl_entry(&self, val: &Value) -> Result<Option<PartsEntry>> {
        parse_rstbl_entry_common(val, "HairFront", VANILLA_MAX_PARTS)
    }

    fn parse_pack_entry(&self, map: &BTreeMap<String, Value>) -> Result<Option<PartsEntry>> {
        parse_pack_entry_common(map, VANILLA_MAX_PARTS)
    }

    fn new_entry(
        &self,
        index: u32,
        model_fmdb: String,
        editor_icon_name: Option<String>,
        rstbl_template: &BTreeMap<String, Value>,
    ) -> PartsEntry {
        let file_name = self.part_name(index);
        let row_id = self.row_id(index);
        let icon = editor_icon_name.unwrap_or_else(|| self.vanilla_icon_fallback().to_string());

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

        let template_index = match rstbl_template.get("PartsIndex") {
            Some(Value::I32(n)) => *n as u32,
            Some(Value::U32(n)) => *n,
            _ => 1,
        };

        if !model_fmdb.is_empty() {
            let internal_mesh_name = model_fmdb
                .split('/')
                .last()
                .unwrap_or("")
                .trim_end_matches(".fmdb");

            let base_path = format!(
                "Work/Model/Mii/MiiHairFront/MiiHairFront{:03}/output/{}.fmdb",
                template_index, internal_mesh_name
            );

            for map in [&mut rstbl_raw, &mut pack_raw] {
                let mut mu = BTreeMap::new();
                mu.insert("Fmdb".into(), Value::String(base_path.clone()));
                mu.insert("PhivePhcl".into(), Value::String("".into()));
                map.insert("ModelUnit".into(), Value::Dict(mu));

                let mut muh = BTreeMap::new();
                muh.insert("Fmdb".into(), Value::String(base_path.clone()));
                muh.insert("PhivePhcl".into(), Value::String("".into()));
                map.insert("ModelUnitForHat".into(), Value::Dict(muh));
            }
        }

        PartsEntry {
            parts_index: index as i32,
            file_name,
            row_id,
            editor_icon_name: Some(icon),
            model_paths: vec![],
            rstbl_raw,
            pack_raw,
        }
    }
}
