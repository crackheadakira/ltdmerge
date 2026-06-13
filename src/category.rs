use anyhow::Result;
use std::collections::BTreeMap;
use tomolib::formats::byml::Value;

#[derive(Debug, Clone)]
pub struct PartsEntry {
    pub parts_index: i32,
    pub file_name: String,
    pub row_id: String,

    /// Value of "EditorIconName" in both documents.
    ///
    /// `None` means the entry had no icon or we should use the vanilla fallback.
    pub editor_icon_name: Option<String>,

    /// The primary model path strings that need index-renaming.

    /// On remap we run a string-replace across all of them simultaneously.
    pub model_paths: Vec<String>,

    /// The full RSTBL dict as parsed.
    pub rstbl_raw: BTreeMap<String, Value>,

    /// The full pack bgyml dict as parsed from MiiParts.pack.
    pub pack_raw: BTreeMap<String, Value>,
}

impl PartsEntry {
    pub fn remap_to(
        &mut self,
        new_index: i32,
        new_file_name: String,
        new_row_id: String,
        old_model_token: &str,
        new_model_token: &str,
        cat: &dyn CategoryDef,
    ) {
        self.parts_index = new_index;
        self.file_name = new_file_name;
        self.row_id = new_row_id;

        if !old_model_token.is_empty() {
            for p in &mut self.model_paths {
                *p = p.replace(old_model_token, new_model_token);
            }
        }

        self.editor_icon_name = Some(cat.editor_icon_name(new_index as u32));

        if let Some(Value::String(icon)) = self.rstbl_raw.get_mut("EditorIconName") {
            *icon = cat.editor_icon_name(new_index as u32);
        }

        if let Some(Value::String(icon)) = self.pack_raw.get_mut("EditorIconName") {
            *icon = cat.editor_icon_name(new_index as u32);
        }

        self.flush_to_raw(old_model_token, new_model_token);
    }

    /// Write the engine-owned fields back into `rstbl_raw` and `pack_raw` safely.
    pub fn flush_to_raw(&mut self, old_model_token: &str, new_model_token: &str) {
        for map in [&mut self.rstbl_raw, &mut self.pack_raw] {
            if map.is_empty() {
                continue;
            }
            map.insert("FileName".into(), Value::String(self.file_name.clone()));
            map.insert("PartsIndex".into(), Value::I32(self.parts_index));

            if let Some(ref icon) = self.editor_icon_name {
                map.insert("EditorIconName".into(), Value::String(icon.clone()));
            }
        }

        if !self.rstbl_raw.is_empty() {
            self.rstbl_raw
                .insert("__RowId".into(), Value::String(self.row_id.clone()));
        }

        for map in [&mut self.rstbl_raw, &mut self.pack_raw] {
            if map.is_empty() {
                continue;
            }

            for sub_val in map.values_mut() {
                Self::smart_path_replace(
                    sub_val,
                    old_model_token,
                    new_model_token,
                    self.parts_index,
                );
            }
        }
    }

    fn smart_path_replace(val: &mut Value, old: &str, new: &str, new_index: i32) {
        match val {
            Value::String(s) => {
                if !old.is_empty() && s.contains(old) {
                    *s = s.replace(old, new);
                } else if old.is_empty()
                    && (s.contains("Work/Model/") || s.contains(".fmdb") || s.contains(".phcl"))
                {
                    *s = Self::rebuild_path_digits(s, new_index);
                }
            }
            Value::Dict(map) => {
                for sub_val in map.values_mut() {
                    Self::smart_path_replace(sub_val, old, new, new_index);
                }
            }
            Value::Array(arr) => {
                for sub_val in arr.iter_mut() {
                    Self::smart_path_replace(sub_val, old, new, new_index);
                }
            }
            _ => {}
        }
    }

    /// Extracts the digits from a asset path component and replaces them with the accurate target index formatting
    fn rebuild_path_digits(path: &str, new_index: i32) -> String {
        let segments: Vec<&str> = path.split('/').collect();
        let mut updated_segments = Vec::new();

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

                let formatted_idx = if prefix.contains("HairAll") {
                    format!("{:03}", new_index)
                } else {
                    format!("{:02}", new_index)
                };

                updated_segments.push(format!("{}{}{}", prefix, formatted_idx, suffix));
            } else {
                updated_segments.push(segment.to_string());
            }
        }

        updated_segments.join("/")
    }

    pub fn build_rstbl_value(&self) -> Value {
        Value::Dict(self.rstbl_raw.clone())
    }

    pub fn build_pack_value(&self) -> Value {
        Value::Dict(self.pack_raw.clone())
    }
}

pub trait CategoryDef: Send + Sync {
    fn category_name(&self) -> &str;
    fn parts_type_hash(&self) -> u32;
    fn vanilla_max_parts_index(&self) -> i32;
    fn part_name(&self, index: u32) -> String;
    fn row_id(&self, index: u32) -> String;

    fn vanilla_icon_fallback(&self) -> &str;

    fn matches_icon_name(&self, tex_name: &str) -> bool;
    fn editor_icon_name(&self, index: u32) -> String;
    fn path_parts_order(&self) -> &str;

    /// Checks if a global BNTX texture name pattern belongs to this category.
    fn matches_texture_name(&self, _tex_name: &str) -> bool {
        false
    }

    fn pack_path(&self, file_name: &str) -> String;
    fn internal_model_name(&self, index: u32) -> String;

    /// Parse one element of the flat rstbl.byml array.
    fn parse_rstbl_entry(&self, val: &Value) -> Result<Option<PartsEntry>>;

    /// Parse the root dict of an inner pack bgyml loaded from MiiParts.pack.
    fn parse_pack_entry(&self, map: &BTreeMap<String, Value>) -> Result<Option<PartsEntry>>;

    /// Build the rstbl Value for `entry`.
    fn build_rstbl_value(&self, entry: &PartsEntry) -> Value {
        build_from_raw(&entry.rstbl_raw, entry)
    }

    /// Build the pack bgyml Value for `entry`.
    fn build_pack_value(&self, entry: &PartsEntry) -> Value {
        build_from_raw(&entry.pack_raw, entry)
    }

    /// Build a brand-new PartsEntry for the `add` command.
    fn new_entry(
        &self,
        index: u32,
        model_fmdb: String,
        editor_icon_name: Option<String>,
        rstbl_template: &BTreeMap<String, Value>,
    ) -> PartsEntry {
        let file_name = self.part_name(index);
        let row_id = self.row_id(index);
        let icon = editor_icon_name
            .clone()
            .unwrap_or_else(|| self.vanilla_icon_fallback().to_string());

        let mut rstbl_raw = rstbl_template.clone();
        rstbl_raw.insert(
            "Category".into(),
            Value::String(self.category_name().into()),
        );

        rstbl_raw.insert("FileName".into(), Value::String(file_name.clone()));
        rstbl_raw.insert("PartsIndex".into(), Value::I32(index as i32));
        rstbl_raw.insert("__RowId".into(), Value::String(row_id.clone()));
        rstbl_raw.insert("EditorIconName".into(), Value::String(icon.clone()));

        if !model_fmdb.is_empty() {
            if let Some(Value::Dict(mu)) = rstbl_raw.get_mut("ModelUnit") {
                mu.insert("Fmdb".into(), Value::String(model_fmdb.clone()));
            }
        }

        let mut pack_raw = BTreeMap::new();
        pack_raw.insert(
            "Category".into(),
            Value::String(self.category_name().into()),
        );

        pack_raw.insert("EditorIconName".into(), Value::String(icon));
        pack_raw.insert("FileName".into(), Value::String(file_name.clone()));
        pack_raw.insert("IsVisibleInEditor".into(), Value::Bool(true));
        pack_raw.insert("PartsIndex".into(), Value::I32(index as i32));

        if !model_fmdb.is_empty() {
            let mut mu = BTreeMap::new();
            mu.insert("Fmdb".into(), Value::String(model_fmdb.clone()));
            pack_raw.insert("ModelUnit".into(), Value::Dict(mu));
        }

        PartsEntry {
            parts_index: index as i32,
            file_name,
            row_id,
            editor_icon_name,
            model_paths: if model_fmdb.is_empty() {
                vec![]
            } else {
                vec![model_fmdb]
            },
            rstbl_raw,
            pack_raw,
        }
    }
}

pub fn extract_entry_fields(
    map: &BTreeMap<String, Value>,
) -> (String, String, Option<String>, Vec<String>) {
    let file_name = str_field(map, "FileName");
    let row_id = str_field(map, "__RowId");

    let editor_icon_name = match map.get("EditorIconName") {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    };

    let mut model_paths = Vec::new();

    collect_model_paths_recursive(Value::Dict(map.clone()), &mut model_paths);

    (file_name, row_id, editor_icon_name, model_paths)
}

fn collect_model_paths_recursive(val: Value, paths: &mut Vec<String>) {
    match val {
        Value::Dict(map) => {
            for (key, sub_val) in map {
                if key == "Fmdb" || key == "PhivePhcl" {
                    if let Value::String(s) = sub_val {
                        if !s.is_empty() {
                            paths.push(s);
                        }
                    }
                } else {
                    collect_model_paths_recursive(sub_val, paths);
                }
            }
        }
        Value::Array(arr) => {
            for sub_val in arr {
                collect_model_paths_recursive(sub_val, paths);
            }
        }
        _ => {}
    }
}

pub fn parse_rstbl_entry_common(
    val: &Value,
    category: &str,
    vanilla_max: i32,
) -> Result<Option<PartsEntry>> {
    let Value::Dict(map) = val else {
        return Ok(None);
    };

    if map.get("Category").and_then(|v| match v {
        Value::String(s) => Some(s.as_str()),
        _ => None,
    }) != Some(category)
    {
        return Ok(None);
    }

    let parts_index = match i32_field(map, "PartsIndex") {
        Some(n) => n,
        None => return Ok(None),
    };

    if parts_index <= vanilla_max {
        return Ok(None);
    }

    let (file_name, row_id, editor_icon_name, model_paths) = extract_entry_fields(map);

    if file_name.is_empty() {
        return Ok(None);
    }

    Ok(Some(PartsEntry {
        parts_index,
        file_name,
        row_id,
        editor_icon_name,
        model_paths,
        rstbl_raw: map.clone(),
        pack_raw: BTreeMap::new(),
    }))
}

pub fn parse_pack_entry_common(
    map: &BTreeMap<String, Value>,
    vanilla_max: i32,
) -> Result<Option<PartsEntry>> {
    let parts_index = match i32_field(map, "PartsIndex") {
        Some(n) => n,
        None => return Ok(None),
    };

    if parts_index <= vanilla_max {
        return Ok(None);
    }

    let (file_name, _row_id, editor_icon_name, model_paths) = extract_entry_fields(map);

    Ok(Some(PartsEntry {
        parts_index,
        file_name,
        row_id: String::new(),
        editor_icon_name,
        model_paths,
        rstbl_raw: BTreeMap::new(),
        pack_raw: map.clone(),
    }))
}

pub fn build_from_raw(raw: &BTreeMap<String, Value>, entry: &PartsEntry) -> Value {
    let mut map = raw.clone();
    map.insert("FileName".into(), Value::String(entry.file_name.clone()));
    map.insert("PartsIndex".into(), Value::I32(entry.parts_index));

    if let Some(ref icon) = entry.editor_icon_name {
        map.insert("EditorIconName".into(), Value::String(icon.clone()));
    }

    if !entry.row_id.is_empty() {
        map.insert("__RowId".into(), Value::String(entry.row_id.clone()));
    }

    Value::Dict(map)
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
