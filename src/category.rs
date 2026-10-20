use crate::typed_param::{self, BymlDecode, BymlOrigin};
use anyhow::{Context, Result};
use ltdmerge_derive::{FromByml, ToByml};
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};
use tomolib::formats::byml::Value;

#[derive(Debug, Clone, ToByml, FromByml, Deserialize, JsonSchema)]
#[serde(default)]
pub struct PartsEntry {
    #[serde(skip)]
    #[schemars(skip)]
    #[byml(skip)]
    pub(crate) origin: BymlOrigin,

    #[byml(key = "__RowId")]
    pub row_id: String,

    pub category: String,
    pub file_name: String,

    #[byml(default = -1)]
    pub parts_index: i32,

    // TODO: make enum, but just mmh3 of category
    #[byml(default = 0)]
    pub parts_type: u32,

    #[byml(default = false)]
    pub is_visible_in_editor: bool,

    #[byml(default = true)]
    pub is_selectable_color: bool,

    #[byml(default = false)]
    pub is_selectable_sub_color: bool,

    pub editor_icon_name: Option<String>,

    #[byml(skip_none)]
    pub editor_mask_icon_name: Option<String>,

    #[byml(via = "hash_u32", default = 0)]
    pub hair_gender: HairGender,

    #[byml(skip_none)]
    pub baby_hair_name_father_male: Option<String>,

    #[byml(skip_none)]
    pub baby_hair_name_father_female: Option<String>,

    #[byml(skip_none)]
    pub baby_hair_name_mother_male: Option<String>,

    #[byml(skip_none)]
    pub baby_hair_name_mother_female: Option<String>,

    #[byml(default = false)]
    pub is_use_hair_parts_model: bool,

    #[byml(skip_none)]
    pub texture_name: Option<String>,

    #[byml(skip_none)]
    pub no_lip_texture_name: Option<String>,

    #[byml(default = false)]
    pub use_texture_color: bool,

    #[byml(default = false)]
    pub is_flippable: bool,

    #[byml(default = false)]
    pub is_attachable_hair_front: bool,

    #[byml(default = false)]
    pub is_attachable_hair_parts_upper: bool,

    #[byml(default = false)]
    pub is_attachable_hair_parts_middle: bool,

    #[byml(default = false)]
    pub is_attachable_hair_parts_lower: bool,

    #[byml(default = false)]
    pub is_mouth_open: bool,

    #[byml(default = false)]
    pub is_enable_mouth_lip_default: bool,

    #[byml(default = Vector2f::default())]
    pub rotate_axis: Vector2f,

    #[byml(default = 0)]
    pub offset_rotate: i32,

    #[byml(default = 18)]
    pub max_trans_x: i32,

    #[byml(default = 0)]
    pub min_trans_y: i32,

    #[byml(default = 31)]
    pub max_trans_y: i32,

    #[byml(default = 0)]
    pub default_scale: i32,

    #[byml(default = 3)]
    pub default_aspect: i32,

    #[byml(default = 0)]
    pub default_trans_x: i32,

    #[byml(default = 0)]
    pub default_trans_y: i32,

    pub axis_for_expression: Vector2f,

    pub size_for_expression: f32,

    #[byml(default = BTreeMap::new())]
    pub components: BTreeMap<String, String>,

    #[byml(default = BTreeMap::new())]
    pub components_hash: BTreeMap<String, u32>,

    #[byml(default = PartsModelUnitParam::default())]
    pub model_unit: PartsModelUnitParam,

    #[byml(skip_none)]
    pub model_unit_for_hat: Option<PartsModelUnitParam>,

    #[byml(skip_none)]
    pub flipped_model_unit: Option<PartsModelUnitParam>,

    #[byml(skip_none)]
    pub flipped_model_unit_for_hat: Option<PartsModelUnitParam>,

    #[byml(default = BTreeMap::new())]
    pub hair_parts_model_unit: BTreeMap<HairPartType, PartsModelUnitParam>,

    #[byml(default = BTreeMap::new())]
    pub hair_parts_model_unit_for_hat: BTreeMap<HairPartType, PartsModelUnitParam>,

    #[byml(default = Vec::new())]
    pub hair_parts_attach_info: Vec<HairPartsAttachInfo>,

    #[byml(default = false)]
    pub is_ltdmerge_gen: bool,
}

impl Default for PartsEntry {
    fn default() -> Self {
        Self {
            origin: BymlOrigin::default(),
            row_id: String::new(),
            category: String::new(),
            file_name: String::new(),
            parts_index: -1,
            parts_type: 0,
            is_visible_in_editor: false,
            is_selectable_color: true,
            is_selectable_sub_color: false,
            editor_icon_name: None,
            editor_mask_icon_name: None,
            hair_gender: HairGender::Both,
            baby_hair_name_father_male: None,
            baby_hair_name_father_female: None,
            baby_hair_name_mother_male: None,
            baby_hair_name_mother_female: None,
            is_use_hair_parts_model: false,
            texture_name: None,
            no_lip_texture_name: None,
            use_texture_color: false,
            is_flippable: false,
            is_attachable_hair_front: false,
            is_attachable_hair_parts_upper: false,
            is_attachable_hair_parts_middle: false,
            is_attachable_hair_parts_lower: false,
            is_mouth_open: false,
            is_enable_mouth_lip_default: false,
            rotate_axis: Vector2f { x: 0.0, y: 0.0 },
            offset_rotate: 0,
            max_trans_x: 18,
            min_trans_y: 0,
            max_trans_y: 31,
            default_scale: 0,
            default_aspect: 3,
            default_trans_x: 0,
            default_trans_y: 0,
            axis_for_expression: Vector2f { x: 0.0, y: 0.0 },
            size_for_expression: 1.0,
            components: BTreeMap::new(),
            components_hash: BTreeMap::new(),
            model_unit: PartsModelUnitParam {
                fmdb: None,
                phcl: None,
            },
            model_unit_for_hat: None,
            flipped_model_unit: None,
            flipped_model_unit_for_hat: None,
            hair_parts_model_unit: BTreeMap::new(),
            hair_parts_model_unit_for_hat: BTreeMap::new(),
            hair_parts_attach_info: Vec::new(),
            is_ltdmerge_gen: false,
        }
    }
}

impl PartsEntry {
    pub fn with_category_defaults(self, index: u32, cat: &dyn CategoryDef) -> Result<Self> {
        let original = self.to_byml_full_map();
        let origin = self.origin.clone();
        let mut entry = cat.apply_category_defaults(index, self);
        let mut values = entry.to_byml_full_map();

        for key in &origin.manifest_fields {
            if [
                "PartsIndex",
                "PartsType",
                "Category",
                "FileName",
                "__RowId",
                "IsLtdmergeGen",
            ]
            .contains(&key.as_str())
            {
                continue;
            }

            match original.get(key) {
                Some(value) => {
                    values.insert(key.clone(), value.clone());
                }
                None => {
                    values.remove(key);
                }
            }
        }

        entry = Self::decode(&Value::Dict(values))?;
        entry.origin = origin;
        entry.sync_component_hashes();

        Ok(entry)
    }

    pub fn sync_component_hashes(&mut self) {
        for (name, path) in &self.components {
            self.components_hash
                .insert(name.clone(), typed_param::murmur32(path));
        }
    }

    pub fn validate(&self) -> Result<()> {
        typed_param::validate("mii__Parts", &self.to_byml_full_map())?;
        anyhow::ensure!(self.parts_index >= 0, "negative PartsIndex");

        for (name, path) in &self.components {
            anyhow::ensure!(
                self.components_hash.get(name) == Some(&typed_param::murmur32(path)),
                "component reference/hash mismatch: {name}"
            );
        }

        Ok(())
    }

    pub fn from_byml_map(
        map: &BTreeMap<String, Value>,
        category_name: &str,
        _vanilla_max: i32,
    ) -> Result<Option<Self>> {
        if !matches!(map.get("Category"), Some(Value::String(s)) if s == category_name) {
            return Ok(None);
        }

        if !matches!(map.get("IsLtdmergeGen"), Some(Value::Bool(true))) {
            return Ok(None);
        }

        typed_param::validate("mii__Parts", map)?;
        let expanded = typed_param::materialize("mii__Parts", map);
        let mut entry = Self::decode(&Value::Dict(expanded))?;
        anyhow::ensure!(
            entry.parts_index >= 0,
            "marked part has a negative PartsIndex"
        );
        entry.origin = BymlOrigin::loaded(map.clone(), entry.to_byml_full_map());
        Ok(Some(entry))
    }

    pub fn remap_to(
        &mut self,
        new_index: i32,
        new_file_name: String,
        new_row_id: String,
        old_model_token: &str,
        new_model_token: &str,
        cat: &dyn CategoryDef,
    ) {
        let old_index = self.parts_index;
        if new_index == old_index {
            return;
        }

        let old_icon = cat.editor_icon_name(old_index as u32);
        if self.editor_icon_name.as_deref() == Some(old_icon.as_str()) {
            self.editor_icon_name = Some(cat.editor_icon_name(new_index as u32));
        }

        if let Some(old_mask) = cat.editor_mask_icon_name(old_index as u32)
            && self.editor_mask_icon_name.as_deref() == Some(old_mask.as_str())
        {
            self.editor_mask_icon_name = cat.editor_mask_icon_name(new_index as u32);
        }

        if self.texture_name.as_deref() == Some(old_model_token) {
            self.texture_name = Some(new_model_token.into());
        }

        if self.no_lip_texture_name.as_deref() == Some(old_model_token) {
            self.no_lip_texture_name = Some(new_model_token.into());
        }

        self.parts_index = new_index;
        self.file_name = new_file_name;
        self.row_id = new_row_id;

        self.flush_to_raw(old_model_token, new_model_token, cat);
    }

    /// Runs smart_path_replace across all model unit path fields.
    pub fn flush_to_raw(
        &mut self,
        old_model_token: &str,
        new_model_token: &str,
        _cat: &dyn CategoryDef,
    ) {
        let index = self.parts_index;

        let mutate_path = |opt_path: &mut Option<String>| {
            if let Some(path) = opt_path {
                let mut temp_val = Value::String(path.clone());
                Self::smart_path_replace(
                    &mut temp_val,
                    old_model_token,
                    new_model_token,
                    index,
                    false,
                );
                if let Value::String(updated) = temp_val {
                    *path = updated;
                }
            }
        };

        mutate_path(&mut self.model_unit.fmdb);
        mutate_path(&mut self.model_unit.phcl);

        if let Some(ref mut unit) = self.model_unit_for_hat {
            mutate_path(&mut unit.fmdb);
            mutate_path(&mut unit.phcl);
        }

        if let Some(ref mut unit) = self.flipped_model_unit {
            mutate_path(&mut unit.fmdb);
            mutate_path(&mut unit.phcl);
        }

        if let Some(ref mut unit) = self.flipped_model_unit_for_hat {
            mutate_path(&mut unit.fmdb);
            mutate_path(&mut unit.phcl);
        }

        for unit in self.hair_parts_model_unit.values_mut() {
            mutate_path(&mut unit.fmdb);
            mutate_path(&mut unit.phcl);
        }

        for unit in self.hair_parts_model_unit_for_hat.values_mut() {
            mutate_path(&mut unit.fmdb);
            mutate_path(&mut unit.phcl);
        }
    }

    fn smart_path_replace(
        val: &mut Value,
        old: &str,
        new: &str,
        new_index: i32,
        force_digit_rebuild: bool,
    ) {
        match val {
            Value::String(s) => {
                if !old.is_empty() && s.contains(old) {
                    if s.ends_with(".fmdb")
                        && let Some((directory, stem)) = s.rsplit_once('/')
                    {
                        *s = format!("{}/{}", directory.replace(old, new), stem);
                    } else {
                        *s = s.replace(old, new);
                    }
                } else if old.is_empty()
                    && (force_digit_rebuild
                        || s.contains("Work/Model/")
                        || s.contains(".fmdb")
                        || s.contains(".phcl"))
                {
                    *s = Self::rebuild_path_digits(s, new_index);
                }
            }

            Value::Dict(map) => {
                for sub_val in map.values_mut() {
                    Self::smart_path_replace(sub_val, old, new, new_index, false);
                }
            }

            Value::Array(arr) => {
                for sub_val in arr.iter_mut() {
                    Self::smart_path_replace(sub_val, old, new, new_index, false);
                }
            }
            _ => {}
        }
    }

    fn rebuild_path_digits(path: &str, new_index: i32) -> String {
        let segments: Vec<&str> = path.split('/').collect();
        let mut updated = Vec::new();

        for segment in segments {
            if segment.starts_with("MiiHead") || segment.starts_with("MiiHair") {
                let prefix: String = segment
                    .chars()
                    .take_while(|c| !c.is_ascii_digit())
                    .collect();

                let suffix: String = segment
                    .chars()
                    .skip_while(|c| !c.is_ascii_digit())
                    .skip_while(|c| c.is_ascii_digit())
                    .collect();

                let formatted = if prefix.contains("HairAll") {
                    format!("{:03}", new_index)
                } else {
                    format!("{:02}", new_index)
                };

                updated.push(format!("{}{}{}", prefix, formatted, suffix));
            } else {
                updated.push(segment.to_string());
            }
        }

        updated.join("/")
    }

    pub fn build_rstbl_value(&self) -> Value {
        self.origin
            .write(self.to_byml_full_map())
            .unwrap_or_else(|| typed_param::row(self.to_byml_full_map()))
    }

    pub fn build_pack_value(&self) -> Value {
        let current = self.to_byml_full_map();
        if let Some(Value::Dict(mut map)) = self.origin.write(current.clone()) {
            for field in ["__RowId", "PartsType"] {
                if !self.origin.originally_contains(field) {
                    map.remove(field);
                }
            }
            Value::Dict(map)
        } else {
            typed_param::packed(current)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[repr(u32)]
pub enum HairGender {
    #[default]
    Both,
    Male,
    Female,
    Unknown(u32),
}

impl HairGender {
    pub fn hash_u32(&self) -> u32 {
        match self {
            Self::Both => 0xa9021713,
            Self::Male => 0x0ddcbe76,
            Self::Female => 0x3b1f8b15,
            Self::Unknown(hash) => *hash,
        }
    }
}

impl BymlDecode for HairGender {
    fn decode(value: &Value) -> Result<Self> {
        Ok(match u32::decode(value)? {
            0xa9021713 => Self::Both,
            0x0ddcbe76 => Self::Male,
            0x3b1f8b15 => Self::Female,
            hash => Self::Unknown(hash),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[repr(u32)]
pub enum HairPartType {
    Right = 0x8eb27afa,
    Both = 0xa9021713,
    Left = 0x63a5d310,
    Center = 0x6e9ad537,

    MiddleLeft = 0x1e4ce47d,
    MiddleRight = 0x2be4a44d,
    MiddleBoth = 0x444fff10,
    MiddleCenter = 0xa5c801e3,

    LowerBoth = 0xb3ed42ab,
    LowerLeft = 0x3e2e5102,
    LowerRight = 0x549acbdc,
    LowerCenter = 0x0d91960a,

    UpperBoth = 0x8f1f2325,
    UpperCenter = 0x9315920f,
    UpperLeft = 0x5eb80c96,
    UpperRight = 0x708fa072,
    Unknown(u32),
}

impl HairPartType {
    pub fn hash_u32(&self) -> u32 {
        match self {
            Self::Right => 0x8eb27afa,
            Self::Both => 0xa9021713,
            Self::Left => 0x63a5d310,
            Self::Center => 0x6e9ad537,
            Self::MiddleLeft => 0x1e4ce47d,
            Self::MiddleRight => 0x2be4a44d,
            Self::MiddleBoth => 0x444fff10,
            Self::MiddleCenter => 0xa5c801e3,
            Self::LowerBoth => 0xb3ed42ab,
            Self::LowerLeft => 0x3e2e5102,
            Self::LowerRight => 0x549acbdc,
            Self::LowerCenter => 0x0d91960a,
            Self::UpperBoth => 0x8f1f2325,
            Self::UpperCenter => 0x9315920f,
            Self::UpperLeft => 0x5eb80c96,
            Self::UpperRight => 0x708fa072,
            Self::Unknown(hash) => *hash,
        }
    }
    fn from_hash(hash: u32) -> Self {
        let known = [
            Self::Right,
            Self::Both,
            Self::Left,
            Self::Center,
            Self::MiddleLeft,
            Self::MiddleRight,
            Self::MiddleBoth,
            Self::MiddleCenter,
            Self::LowerBoth,
            Self::LowerLeft,
            Self::LowerRight,
            Self::LowerCenter,
            Self::UpperBoth,
            Self::UpperCenter,
            Self::UpperLeft,
            Self::UpperRight,
        ];
        known
            .into_iter()
            .find(|k| k.hash_u32() == hash)
            .unwrap_or(Self::Unknown(hash))
    }
}
impl<T: BymlDecode> BymlDecode for BTreeMap<HairPartType, T> {
    fn decode(value: &Value) -> Result<Self> {
        let Value::Hash32(values) = value else {
            anyhow::bail!("expected hashed enum map");
        };
        values
            .iter()
            .map(|(key, value)| Ok((HairPartType::from_hash(*key), T::decode(value)?)))
            .collect()
    }
}

#[derive(Debug, Clone, ToByml, FromByml, Deserialize, JsonSchema, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Vector2f {
    #[byml(default = 0.0)]
    pub x: f32,

    #[byml(default = 0.0)]
    pub y: f32,
}

#[derive(Debug, Clone, ToByml, FromByml, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PartsModelUnitParam {
    #[byml(skip_none)]
    pub fmdb: Option<String>,

    #[byml(key = "PhivePhcl", skip_none)]
    pub phcl: Option<String>,
}

impl PartsModelUnitParam {
    pub fn compile_and_finalize(
        &mut self,
        target_token: &str,
        folder_name: &str,
        romfs_files: &mut BTreeMap<String, Vec<u8>>,
    ) -> Result<()> {
        if let Some(ref raw_model_source) = self.fmdb {
            let model_bytes = crate::util::read_and_decompress(Path::new(""), raw_model_source)?;
            let mut model_bfres = crate::util::bfres_parse(&model_bytes)?;

            let internal_mesh_name = model_bfres
                .models
                .names
                .first()
                .context("The provided BFRES file contains no internal meshes.")?
                .clone();

            model_bfres.name = target_token.to_string();
            let serialized = model_bfres.write().context("BFRES serialization failed")?;

            romfs_files.insert(format!("Model/{target_token}.bfres"), serialized);

            self.fmdb = Some(format!(
                "Work/Model/Mii/MiiHead/{folder_name}/output/{internal_mesh_name}.fmdb"
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, ToByml, FromByml, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct HairPartsAttachInfo {
    pub editor_lower_icon_name: String,
    pub editor_middle_icon_name: String,
    pub editor_upper_icon_name: String,
    pub hair_parts_name: String,
    pub is_attachable_lower: bool,
    pub is_attachable_middle: bool,
    pub is_attachable_upper: bool,
}

#[derive(Debug, Clone, ToByml, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ComponentInfo {
    pub confluence_page_info: u32,
}

pub trait CategoryDef: Send + Sync {
    fn category_name(&self) -> &str;

    fn internal_category_name(&self) -> &str {
        self.category_name()
    }

    fn parts_type_hash(&self) -> u32;
    fn vanilla_max_parts_index(&self) -> i32;
    fn part_name(&self, index: u32) -> String;
    fn row_id(&self, index: u32) -> String;
    fn vanilla_icon_fallback(&self) -> &str;
    fn matches_icon_name(&self, tex_name: &str) -> bool;
    fn editor_icon_name(&self, index: u32) -> String;

    fn editor_mask_icon_name(&self, _index: u32) -> Option<String> {
        None
    }

    fn path_parts_order(&self) -> &str;
    fn pack_path(&self, file_name: &str) -> String;
    fn internal_model_name(&self, index: u32) -> String;

    fn matches_texture_name(&self, _tex_name: &str) -> bool {
        false
    }

    fn extra_index_fields(&self, _index: u32) -> Vec<(&'static str, String)> {
        vec![]
    }

    fn extra_remappable_string_keys(&self) -> &[&'static str] {
        &[]
    }

    fn build_rstbl_value(&self, entry: &PartsEntry) -> Value {
        entry.build_rstbl_value()
    }

    fn build_pack_value(&self, entry: &PartsEntry) -> Value {
        entry.build_pack_value()
    }

    fn apply_category_defaults(&self, index: u32, mut entry: PartsEntry) -> PartsEntry {
        entry.parts_index = index as i32;
        entry.parts_type = self.parts_type_hash();
        entry.category = self.internal_category_name().to_string();
        entry.file_name = self.part_name(index);
        entry.row_id = self.row_id(index);
        entry
    }

    fn json_schema(&self) -> Schema;
}

pub fn str_field(map: &BTreeMap<String, Value>, key: &str) -> String {
    match map.get(key) {
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

pub fn i32_field(map: &BTreeMap<String, Value>, key: &str) -> Option<i32> {
    match map.get(key) {
        Some(Value::I32(n)) => Some(*n),
        Some(Value::U32(n)) => Some(*n as i32),
        _ => None,
    }
}
