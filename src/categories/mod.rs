use crate::category::{AssetKind, CategoryDef, PartsEntry};
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
    pub icon_match: &'static str,
    pub fallback_icon: &'static str,
    pub mask_icon: bool,
    pub order: &'static str,
    pub defaults: fn(&mut PartsEntry),
}

impl CategorySpec {
    const fn new(name: &'static str, width: usize, kind: AssetKind) -> Self {
        Self {
            name,
            category: name,
            stem: name,
            width,
            kind,
            model: "",
            template: "",
            vanilla_max: 0,
            icon_match: "",
            fallback_icon: "",
            mask_icon: false,
            order: name,
            defaults: |_| {},
        }
    }

    fn numbered(&self, stem: &str, index: u32) -> String {
        format!("{stem}{index:0width$}", width = self.width)
    }
}

pub const ALL: &[CategorySpec] = &[
    CategorySpec {
        template: "Eye000",
        vanilla_max: 80,
        icon_match: "MiiEditor_Face_Eye",
        fallback_icon: "MiiEditor_Face_Eye001_Uit",
        mask_icon: true,
        defaults: |entry| {
            entry.components.insert(
                "EyeAccessoryRef".into(),
                "Work/Mii/EyeAccessoryParam/Eye000.mii__EyeAccessoryParam.gyml".into(),
            );
            entry.axis_for_expression.x = 0.38;
            entry.axis_for_expression.y = 0.01;
        },
        ..CategorySpec::new("Eye", 3, AssetKind::Texture)
    },
    CategorySpec {
        model: "MiiEar",
        vanilla_max: 4,
        icon_match: "MiiEditor_Face_Ear",
        fallback_icon: "MiiEditor_Face_Ear00_Uit",
        ..CategorySpec::new("Ear", 2, AssetKind::Model)
    },
    CategorySpec {
        model: "MiiHead",
        vanilla_max: 21,
        icon_match: "MiiEditor_Face_Faceline",
        fallback_icon: "MiiEditor_Face_Faceline15_Uit",
        mask_icon: true,
        defaults: |entry| {
            entry.is_flippable = false;
            entry.is_use_hair_parts_model = false;
            if entry.default_scale == 0 {
                entry.default_scale = 2;
            }
        },
        ..CategorySpec::new("Faceline", 2, AssetKind::Model)
    },
    CategorySpec {
        category: "Hair",
        model: "MiiHairBack",
        vanilla_max: 340,
        icon_match: "MiiEditor_Face_HairBack",
        fallback_icon: "MiiEditor_Face_HairBack001_Uit",
        defaults: |entry| {
            entry.is_attachable_hair_front = true;
            entry.is_attachable_hair_parts_upper = true;
            entry.is_attachable_hair_parts_middle = true;
            entry.is_attachable_hair_parts_lower = true;
        },
        ..CategorySpec::new("HairBack", 3, AssetKind::Model)
    },
    CategorySpec {
        model: "MiiHairFront",
        vanilla_max: 82,
        icon_match: "MiiEditor_Face_HairFront",
        fallback_icon: "MiiEditor_Face_HairFront001_Uit",
        ..CategorySpec::new("HairFront", 3, AssetKind::Model)
    },
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

    fn vanilla_icon_fallback(&self) -> &str {
        self.fallback_icon
    }

    fn path_parts_order(&self) -> String {
        format!("Mii/PartsOrder/{}.mii__PartsOrder.bgyml", self.order)
    }

    fn template_texture(&self) -> Option<&str> {
        (self.kind == AssetKind::Texture).then_some(self.template)
    }

    fn matches_icon_name(&self, tex_name: &str) -> bool {
        tex_name.contains(self.icon_match)
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
        entry.is_selectable_color = true;
        entry.max_trans_x = 18;
        entry.min_trans_y = 0;
        entry.max_trans_y = 31;

        if self.kind == AssetKind::Texture && entry.texture_name.is_none() {
            entry.texture_name = Some(self.part_name(index));
        }

        (self.defaults)(&mut entry);
        entry.is_ltdmerge_gen = true;
        entry
    }

    fn json_schema(&self) -> Schema {
        schemars::schema_for!(PartsEntry)
    }
}
