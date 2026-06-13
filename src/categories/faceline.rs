use crate::category::{CategoryDef, PartsEntry, parse_pack_entry_common, parse_rstbl_entry_common};
use anyhow::Result;
use std::collections::BTreeMap;
use tomolib::formats::byml::Value;

const VANILLA_MAX_PARTS: i32 = 21;
const VANILLA_ENTRY_COUNT: u32 = 15;
const FALLBACK_ICON: &str = "MiiEditor_Face_Faceline15_Uit";

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

    fn vanilla_entry_count(&self) -> u32 {
        VANILLA_ENTRY_COUNT
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

    fn is_vanilla_icon(&self, icon: &str) -> bool {
        if let Some(pos) = icon.find("Faceline") {
            let num: String = icon[pos + 8..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();

            if let Ok(n) = num.parse::<u32>() {
                return n <= self.vanilla_entry_count();
            }
        }
        false
    }

    fn parse_rstbl_entry(&self, val: &Value) -> Result<Option<PartsEntry>> {
        parse_rstbl_entry_common(val, "Faceline", VANILLA_MAX_PARTS)
    }

    fn parse_pack_entry(&self, map: &BTreeMap<String, Value>) -> Result<Option<PartsEntry>> {
        parse_pack_entry_common(map, VANILLA_MAX_PARTS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::CategoryDef;

    fn make_entry(index: u32) -> PartsEntry {
        let cat = FacelineDef;
        let fmdb =
            format!("Work/Model/Mii/MiiHead/MiiHead{index:02}/output/MiiHead{index:02}.fmdb");

        cat.new_entry(
            index,
            fmdb,
            Some(format!("MiiEditor_Face_Faceline{index}_Uit")),
            &BTreeMap::new(),
        )
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
    fn vanilla_icon_detection() {
        let cat = FacelineDef;

        assert!(cat.is_vanilla_icon("MiiEditor_Face_Faceline15_Uit"));
        assert!(!cat.is_vanilla_icon("MiiEditor_Face_Faceline16_Uit"));
        assert!(!cat.is_vanilla_icon("MiiEditor_Face_Faceline22_Uit"));
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
            _ => panic!("pack_raw structural replacement failed to execute"),
        }

        match entry.pack_raw.get("PartsIndex") {
            Some(Value::I32(n)) => assert_eq!(*n, 30),
            _ => panic!("pack_raw PartsIndex not updated"),
        }
    }
}
