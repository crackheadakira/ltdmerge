use crate::category::{CategoryDef, PartsEntry};
use schemars::Schema;

const VANILLA_MAX_PARTS: i32 = 337;
const FALLBACK_ICON: &str = "MiiEditor_Face_HairBack001_Uit";

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

    fn apply_category_defaults(&self, index: u32, mut entry: PartsEntry) -> PartsEntry {
        entry.parts_index = index as i32;
        entry.parts_type = self.parts_type_hash();
        entry.category = self.internal_category_name().to_string();
        entry.file_name = self.part_name(index);
        entry.row_id = self.row_id(index);

        entry.is_visible_in_editor = true;
        entry.is_selectable_color = true;
        entry.is_attachable_hair_front = true;
        entry.is_attachable_hair_parts_upper = true;
        entry.is_attachable_hair_parts_middle = true;
        entry.is_attachable_hair_parts_lower = true;
        entry.max_trans_x = 18;
        entry.min_trans_y = 0;
        entry.max_trans_y = 31;

        entry
    }

    fn json_schema(&self) -> Schema {
        schemars::schema_for!(PartsEntry)
    }
}
