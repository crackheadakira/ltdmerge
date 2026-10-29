use crate::category::{AssetKind, CategoryDef, PartsEntry};
use crate::linked::{EYE_ACCESSORY, LinkedTable};
use crate::typed_param::murmur32;
use schemars::Schema;

#[derive(Clone, Copy)]
pub struct CategorySpec {
    pub name: &'static str,
    pub category: &'static str,
    pub stem: &'static str,
    pub width: usize,
    pub kind: AssetKind,
    pub model: &'static str,
    pub template: &'static str,
    pub vanilla_max: i32,
    pub mask_icon: bool,
    pub order: Option<&'static str>,
    pub linked: &'static [LinkedTable],
}

impl CategorySpec {
    const fn texture(name: &'static str, width: usize, template: &'static str, max: i32) -> Self {
        Self {
            name,
            category: name,
            stem: name,
            width,
            kind: AssetKind::Texture,
            model: "",
            template,
            vanilla_max: max,
            mask_icon: false,
            order: Some(name),
            linked: &[],
        }
    }

    const fn model(
        name: &'static str,
        width: usize,
        model: &'static str,
        template: &'static str,
        max: i32,
    ) -> Self {
        Self {
            kind: AssetKind::Model,
            model,
            ..Self::texture(name, width, template, max)
        }
    }

    const fn unordered(self) -> Self {
        Self {
            order: None,
            ..self
        }
    }

    fn numbered(&self, stem: &str, index: u32) -> String {
        format!("{stem}{index:0width$}", width = self.width)
    }
}

pub const ALL: &[CategorySpec] = &[
    CategorySpec {
        mask_icon: true,
        linked: &[EYE_ACCESSORY],
        ..CategorySpec::texture("Eye", 3, "Eye000", 80)
    },
    CategorySpec::texture("Eyebrow", 2, "Eyebrow00", 42),
    CategorySpec::texture("EyelashUpper", 2, "EyelashUpper00", 5).unordered(),
    CategorySpec::texture("EyelashLower", 2, "EyelashLower00", 1).unordered(),
    CategorySpec::texture("EyelidUpper", 2, "EyelidUpper00", 2).unordered(),
    CategorySpec::texture("EyelidLower", 2, "EyelidLower00", 2).unordered(),
    CategorySpec::texture("Highlight", 2, "Highlight00", 9),
    CategorySpec {
        mask_icon: true,
        ..CategorySpec::texture("Mouth", 3, "Mouth002", 55)
    },
    CategorySpec::texture("Mustache", 2, "Mustache00", 15),
    CategorySpec::texture("BeardShort", 2, "BeardShort00", 14),
    CategorySpec::texture("MakeUpper", 2, "MakeUpper00", 3).unordered(),
    CategorySpec::texture("MakeLower", 2, "MakeLower00", 9),
    CategorySpec::texture("WrinkleUpper", 2, "WrinkleUpper00", 8),
    CategorySpec::texture("WrinkleLower", 2, "WrinkleLower00", 14),
    CategorySpec::texture("Mole", 2, "Mole00", 1).unordered(),
    CategorySpec::model("Ear", 2, "MiiEar", "Ear00", 4),
    CategorySpec {
        mask_icon: true,
        ..CategorySpec::model("Faceline", 2, "MiiHead", "Faceline00", 21)
    },
    CategorySpec {
        category: "Hair",
        ..CategorySpec::model("HairBack", 3, "MiiHairBack", "HairBack006", 340)
    },
    CategorySpec::model("HairFront", 3, "MiiHairFront", "HairFront000", 82),
];

impl CategoryDef for CategorySpec {
    fn category_name(&self) -> &str {
        self.name
    }

    fn internal_category_name(&self) -> &str {
        self.category
    }

    fn parts_type_hash(&self) -> u32 {
        murmur32(self.category)
    }

    fn vanilla_max_parts_index(&self) -> i32 {
        self.vanilla_max
    }

    fn asset_kind(&self) -> AssetKind {
        self.kind
    }

    fn model_folder(&self) -> &str {
        self.model
    }

    fn template_part(&self) -> &str {
        self.template
    }

    fn part_name(&self, index: u32) -> String {
        self.numbered(self.stem, index)
    }

    fn row_id(&self, index: u32) -> String {
        format!("Work/Mii/Parts/{}.mii__Parts.gyml", self.part_name(index))
    }

    fn internal_model_name(&self, index: u32) -> String {
        match self.kind {
            AssetKind::Texture => self.part_name(index),
            _ => self.numbered(self.model, index),
        }
    }

    fn pack_path(&self, file_name: &str) -> String {
        format!("Mii/Parts/{file_name}.mii__Parts.bgyml")
    }

    fn path_parts_order(&self) -> Option<String> {
        self.order
            .map(|order| format!("Mii/PartsOrder/{order}.mii__PartsOrder.bgyml"))
    }

    fn linked_tables(&self) -> &[LinkedTable] {
        self.linked
    }

    fn editor_icon_name(&self, index: u32) -> String {
        format!("MiiEditor_Face_{}_Uit", self.part_name(index))
    }

    fn editor_mask_icon_name(&self, index: u32) -> Option<String> {
        self.mask_icon
            .then(|| format!("MiiEditor_Face_{}color_Uit", self.part_name(index)))
    }

    fn apply_category_defaults(&self, index: u32, mut entry: PartsEntry) -> PartsEntry {
        entry.parts_index = index as i32;
        entry.parts_type = self.parts_type_hash();
        entry.category = self.category.to_string();
        entry.file_name = self.part_name(index);
        entry.row_id = self.row_id(index);
        entry.is_visible_in_editor = true;

        if self.kind == AssetKind::Texture && entry.texture_name.is_none() {
            entry.texture_name = Some(self.part_name(index));
        }

        entry.is_ltdmerge_gen = true;
        entry
    }

    fn json_schema(&self) -> Schema {
        schemars::schema_for!(PartsEntry)
    }
}
