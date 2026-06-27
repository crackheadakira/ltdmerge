use crate::category::{CategoryDef, PartsEntry};
use schemars::Schema;

const VANILLA_MAX_PARTS: i32 = 62;
const FALLBACK_ICON: &str = "MiiEditor_Face_Eye001_Uit";

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

    fn internal_model_name(&self, index: u32) -> String {
        format!("Eye{:03}", index)
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

    fn editor_mask_icon_name(&self, index: u32) -> Option<String> {
        Some(format!("MiiEditor_Face_{}color_Uit", self.part_name(index)))
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

    fn apply_category_defaults(&self, index: u32, mut entry: PartsEntry) -> PartsEntry {
        entry.parts_index = index as i32;
        entry.parts_type = self.parts_type_hash();
        entry.category = self.category_name().to_string();
        entry.file_name = self.part_name(index);
        entry.row_id = self.row_id(index);

        entry.components.insert(
            "EyeAccessoryRef".to_string(),
            "Work/Mii/EyeAccessoryParam/Eye000.mii__EyeAccessoryParam.gyml".to_string(),
        );

        entry
            .components_hash
            .insert("EyeAccessoryRef".to_string(), 0x6207D9B3);

        entry.axis_for_expression.x = 0.38;
        entry.axis_for_expression.y = 0.01;

        entry.is_visible_in_editor = true;
        entry.is_selectable_color = true;

        if entry.texture_name.is_none() {
            entry.texture_name = Some(self.part_name(index));
        }

        entry
    }

    fn json_schema(&self) -> Schema {
        schemars::schema_for!(PartsEntry)
    }
}
