use crate::add::find_template_entry;
use crate::rstbl::alloc_size;
use crate::util::*;
use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tomolib::formats::bfres::Bfres;
use tomolib::formats::bntx::Bntx;
use tomolib::formats::byml::{Byml, Endian, Value};
use tomolib::formats::rstbl::Rstbl;

pub fn run(mod_dirs: Vec<PathBuf>, out_dir: PathBuf) -> Result<()> {
    if mod_dirs.len() < 2 {
        bail!("merge requires at least two input mod directories");
    }

    println!("Merging {} mods → {}", mod_dirs.len(), out_dir.display());

    let mut custom_mods = Vec::new();
    for (idx, dir) in mod_dirs.iter().enumerate() {
        if let Some(loaded_mod) = CustomFacelineMod::load(idx, dir)? {
            custom_mods.push(loaded_mod);
        }
    }

    let mut engine = FacelineMergeEngine::new(custom_mods)?;
    engine.compute_allocations()?;
    engine.execute_remap()?;

    let mut written_sizes: BTreeMap<String, usize> = BTreeMap::new();

    let rstbl_rel_path = Path::new("romfs").join(PATH_RSTBL_BYML);

    if let Some(base_mod_dir) = mod_dirs
        .iter()
        .rev()
        .find(|d| d.join(&rstbl_rel_path).exists())
    {
        let raw_bytes = read_and_decompress(base_mod_dir, rstbl_rel_path.to_str().unwrap())?;
        let mut doc = byml_parse(&raw_bytes)?;
        let arr = byml_root_array_mut(&mut doc)?;

        let mut claimed_by_mods = BTreeSet::new();
        for ((_, old_idx), new_idx) in &engine.allocation_map {
            claimed_by_mods.insert(*old_idx);
            claimed_by_mods.insert(*new_idx);
        }

        arr.retain(|val| {
            if let Value::Dict(map) = val {
                let is_faceline = match map.get("Category") {
                    Some(Value::String(s)) => s == "Faceline",
                    _ => false,
                };

                if !is_faceline {
                    return true;
                }

                let idx = match map.get("PartsIndex") {
                    Some(Value::I32(n)) => Some(*n),
                    Some(Value::U32(n)) => Some(*n as i32),
                    _ => None,
                };

                if let Some(index) = idx {
                    if claimed_by_mods.contains(&index) {
                        return false;
                    }
                }
            }

            true
        });

        for custom_mod in &engine.mods {
            for entry in &custom_mod.rstbl_entries {
                arr.push(entry.to_byml_value(&custom_mod.pack_template))
            }
        }

        arr.sort_by(|a, b| {
            fn get_row_id(val: &Value) -> &str {
                if let Value::Dict(m) = val {
                    if let Some(Value::String(s)) = m.get("__RowId") {
                        return s.as_str();
                    }
                }

                ""
            }

            get_row_id(a).cmp(get_row_id(b))
        });

        let merged_bytes = byml_to_bytes(&doc)?;
        let size = compress_and_write(&out_dir, rstbl_rel_path.to_str().unwrap(), &merged_bytes)?;
        written_sizes.insert(clean_name(PATH_RSTBL_BYML).to_string(), size);
    }

    let pack_rel_path = Path::new("romfs").join(PATH_MII_PARTS_PACK);
    if let Some(base_mod_dir) = mod_dirs
        .iter()
        .rev()
        .find(|d| d.join(&pack_rel_path).exists())
    {
        let raw_bytes = read_and_decompress(base_mod_dir, pack_rel_path.to_str().unwrap())?;
        let base_sarc = sarc_parse(raw_bytes)?;
        let mut merged_entries = sarc_entries_owned(&base_sarc);

        let mut files_to_remove = BTreeSet::new();
        for ((_, old_idx), new_idx) in &engine.allocation_map {
            files_to_remove.insert(format!("Mii/Parts/Faceline{}.mii__Parts.bgyml", old_idx));
            files_to_remove.insert(format!("Mii/Parts/Faceline{}.mii__Parts.bgyml", new_idx));
        }

        merged_entries.retain(|key, _| !files_to_remove.contains(key));

        for custom_mod in &engine.mods {
            for (parts_idx, part) in &custom_mod.pack_parts {
                let sarc_internal_key = format!("Mii/Parts/Faceline{parts_idx}.mii__Parts.bgyml");
                let part_byml_value = part.to_byml_value();

                let doc_wrapper = Byml {
                    root: part_byml_value,
                    endian: Endian::Little,
                    version: 7,
                };

                merged_entries.insert(sarc_internal_key, byml_to_bytes(&doc_wrapper)?);
            }
        }

        let packed_sarc_bytes = sarc_pack(&merged_entries, &base_sarc)?;
        let size = compress_and_write(
            &out_dir,
            pack_rel_path.to_str().unwrap(),
            &packed_sarc_bytes,
        )?;

        written_sizes.insert(clean_name(PATH_MII_PARTS_PACK).to_string(), size);
    }

    let order_rel_path = Path::new("romfs").join(PATH_PARTS_ORDER);
    if let Some(base_mod_dir) = mod_dirs
        .iter()
        .rev()
        .find(|d| d.join(&order_rel_path).exists())
    {
        let raw_bytes = read_and_decompress(base_mod_dir, order_rel_path.to_str().unwrap())?;
        let mut doc = byml_parse(&raw_bytes)?;

        let mut unified_order = parse_parts_order_array(&raw_bytes)?;

        for custom_mod in &engine.mods {
            for &custom_idx in &custom_mod.custom_order_indices {
                if !unified_order.contains(&custom_idx) {
                    unified_order.push(custom_idx);
                }
            }
        }

        let array_values: Vec<Value> = unified_order
            .into_iter()
            .map(|idx| Value::I32(idx))
            .collect();
        byml_root_dict_mut(&mut doc)?.insert("Order".to_string(), Value::Array(array_values));

        let ordered_bytes = byml_to_bytes(&doc)?;
        let size = compress_and_write(&out_dir, order_rel_path.to_str().unwrap(), &ordered_bytes)?;
        written_sizes.insert(clean_name(PATH_PARTS_ORDER).to_string(), size);
    }

    for custom_mod in &engine.mods {
        for (head_name, bfres) in &custom_mod.models {
            let out_rel_path = format!("romfs/Model/{head_name}.bfres.zs");
            let raw_bfres_payload = bfres.write().context("BFRES layout serialization failed")?;

            let size = compress_and_write(&out_dir, &out_rel_path, &raw_bfres_payload)?;

            let rstbl_lookup_key = format!("Model/{head_name}.bfres");
            written_sizes.insert(rstbl_lookup_key, size);
        }
    }

    for dir in &mod_dirs {
        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let rel_path = entry.path().strip_prefix(dir)?;
            let rel_str = rel_path.to_str().unwrap_or("");
            let clean = clean_name(rel_str);

            if clean.starts_with("Model/") && clean.ends_with(".bfres") {
                if let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) {
                    let num_str: String = stem.chars().filter(|c| c.is_ascii_digit()).collect();

                    if let Ok(parts_idx) = num_str.parse::<i32>() {
                        if parts_idx <= VANILLA_INDICES {
                            let raw_bytes = read_and_decompress(dir, rel_str)?;
                            let size = compress_and_write(&out_dir, rel_str, &raw_bytes)?;

                            written_sizes.insert(clean.to_string(), size);
                        }
                    }
                }
            }
        }
    }

    let icon_rel_path = Path::new("romfs").join(PATH_MII_EDITOR_ICON);
    if let Some(base_mod) = engine.mods.iter().find(|m| m.custom_icon.is_some()) {
        let mut base_bntx = base_mod
            .custom_icon
            .clone()
            .context("BNTX missing despite flag")?;

        for custom_mod in &engine.mods {
            if custom_mod.mod_index == base_mod.mod_index {
                continue;
            }

            if let Some(ref mod_bntx) = custom_mod.custom_icon {
                for tex in &mod_bntx.textures {
                    let name_lower = tex.name.to_lowercase();

                    if name_lower.contains("faceline") {
                        let num_str: String =
                            tex.name.chars().filter(|c| c.is_ascii_digit()).collect();

                        if let Ok(idx) = num_str.parse::<i32>() {
                            if idx > VANILLA_INDICES {
                                println!("Merging custom texture icon entry: {}", tex.name);

                                base_bntx.textures.push(tex.clone());
                            }
                        }
                    }
                }
            }
        }

        let merged_bntx_bytes = base_bntx
            .write()
            .context("BNTX layout serialization failed")?;

        let size = compress_and_write(
            &out_dir,
            icon_rel_path.to_str().unwrap(),
            &merged_bntx_bytes,
        )?;

        let rstbl_lookup_key = clean_name(PATH_MII_EDITOR_ICON).to_string();
        written_sizes.insert(rstbl_lookup_key, size);
    }

    let rsizetable_rel = Path::new("romfs").join(PATH_RSIZETABLE);

    if let Some(base_mod_dir) = mod_dirs
        .iter()
        .rev()
        .find(|d| d.join(&rsizetable_rel).exists())
    {
        println!("merging + patching rsizetable");
        let first_bytes = read_and_decompress(base_mod_dir, rsizetable_rel.to_str().unwrap())?;
        let mut merged_tbl = Rstbl::parse(&first_bytes).context("rsizetable parse")?;

        let pack_clean = clean_name(PATH_MII_PARTS_PACK);
        for (clean_path, decompressed_size) in &written_sizes {
            if clean_path == pack_clean {
                continue;
            }
            let calculated = alloc_size(clean_path, *decompressed_size)?;

            merged_tbl.set(clean_path, calculated);
        }

        let mut buf = Vec::new();
        merged_tbl
            .write(&mut buf)
            .context("rsizetable write verification failure")?;
        compress_and_write(&out_dir, rsizetable_rel.to_str().unwrap(), &buf)?;
    }

    println!("Done.");
    Ok(())
}

#[derive(Debug, Clone)]
pub struct PartsProductFaceline {
    pub editor_icon_name: Option<String>,
    pub file_name: String,
    pub model_fmdb: String,
    pub row_id: String,

    pub parts_index: i32,
}

impl PartsProductFaceline {
    pub fn model_str(&self) -> String {
        format!("MiiHead{:02}", self.parts_index)
    }

    fn is_icon_string_vanilla(icon: &str) -> bool {
        if let Some(start) = icon.find("Faceline") {
            let num_part: String = icon[start + 8..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();

            if let Ok(idx) = num_part.parse::<u32>() {
                return idx <= VANILLA_HEADS;
            }
        }
        false
    }

    pub fn from_byml_entry(entry: &Value) -> Result<Option<Self>> {
        let Value::Dict(map) = entry else {
            return Ok(None);
        };

        if map.get("Category").and_then(|v| match v {
            Value::String(s) => Some(s),
            _ => None,
        }) != Some(&"Faceline".to_string())
        {
            return Ok(None);
        };

        let parts_index = match map.get("PartsIndex") {
            Some(Value::I32(idx)) => *idx,
            _ => return Ok(None),
        };

        if parts_index <= VANILLA_INDICES {
            return Ok(None);
        };

        let file_name = map
            .get("FileName")
            .and_then(|v| match v {
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .context("Faceline entry missing 'FileName'")?;

        let row_id = map
            .get("__RowId")
            .and_then(|v| match v {
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .context("Faceline entry missing '__RowId'")?;

        let model_unit = map
            .get("ModelUnit")
            .and_then(|v| match v {
                Value::Dict(d) => Some(d),
                _ => None,
            })
            .context("Faceline entry missing 'ModelUnit' dictionary")?;

        let model_fmdb = model_unit
            .get("Fmdb")
            .and_then(|v| match v {
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .context("ModelUnit missing 'Fmdb' path string")?;

        let editor_icon_name = map
            .get("EditorIconName")
            .and_then(|v| match v {
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .filter(|icon| !Self::is_icon_string_vanilla(icon));

        Ok(Some(Self {
            file_name,
            row_id,
            model_fmdb,
            editor_icon_name,
            parts_index,
        }))
    }

    pub fn remap_to(&mut self, new_index: i32) {
        let old_head = self.model_str();
        self.parts_index = new_index;

        let new_head = self.model_str();

        self.file_name = format!("Faceline{new_index}");
        self.row_id = format!("Work/Mii/Parts/Faceline{new_index}.mii__Parts.gyml");
        self.model_fmdb = self.model_fmdb.replace(&old_head, &new_head);

        if let Some(ref icon) = self.editor_icon_name {
            let old_num = icon
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>();
            if !old_num.is_empty() {
                self.editor_icon_name = Some(icon.replace(&old_num, &new_index.to_string()));
            }
        }
    }

    pub fn to_byml_value(&self, template: &BTreeMap<String, Value>) -> Value {
        let mut root = template.clone();

        root.insert(
            "Category".to_string(),
            Value::String("Faceline".to_string()),
        );

        let icon_str = self
            .editor_icon_name
            .clone()
            .unwrap_or_else(|| "MiiEditor_Face_Faceline15_Uit".to_string());

        root.insert("EditorIconName".to_string(), Value::String(icon_str));

        root.insert(
            "FileName".to_string(),
            Value::String(self.file_name.clone()),
        );

        if let Some(Value::Dict(mu)) = root.get_mut("ModelUnit") {
            mu.insert("Fmdb".to_string(), Value::String(self.model_fmdb.clone()));
        }

        root.insert("PartsIndex".to_string(), Value::I32(self.parts_index));
        root.insert("__RowId".to_string(), Value::String(self.row_id.clone()));

        Value::Dict(root)
    }
}

#[derive(Debug, Clone)]
pub struct FacelineParts {
    pub category: String,
    pub editor_icon_name: String,
    pub file_name: String,
    pub is_visible_in_editor: bool,
    pub model_fmdb: String,
    pub parts_index: i32,
}

impl FacelineParts {
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let doc = byml_parse(data)?;
        let root = byml_root_dict(&doc)?;

        let parts_index = match root.get("PartsIndex") {
            Some(Value::I32(idx)) => *idx,
            _ => bail!("Inner bgyml missing PartsIndex"),
        };

        let file_name = root
            .get("FileName")
            .and_then(|v| match v {
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .context("Inner bgyml missing FileName")?;

        let editor_icon_name = root
            .get("EditorIconName")
            .and_then(|v| match v {
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .context("Inner bgyml missing EditorIconName")?;

        let model_unit = root
            .get("ModelUnit")
            .and_then(|v| match v {
                Value::Dict(d) => Some(d),
                _ => None,
            })
            .context("Inner bgyml missing ModelUnit")?;

        let model_fmdb = model_unit
            .get("Fmdb")
            .and_then(|v| match v {
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .context("Inner ModelUnit missing Fmdb")?;

        Ok(Self {
            category: "Faceline".to_string(),
            editor_icon_name,
            file_name,
            is_visible_in_editor: true,
            model_fmdb,
            parts_index,
        })
    }

    pub fn remap_to(&mut self, old_parts_index: i32, new_index: i32) {
        let old_head = format!("MiiHead{:02}", old_parts_index);
        self.parts_index = new_index;
        let new_head = format!("MiiHead{:02}", new_index);

        self.file_name = format!("Faceline{new_index}");
        self.model_fmdb = self.model_fmdb.replace(&old_head, &new_head);

        if !PartsProductFaceline::is_icon_string_vanilla(&self.editor_icon_name) {
            let old_num = self
                .editor_icon_name
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>();

            if !old_num.is_empty() {
                self.editor_icon_name = self
                    .editor_icon_name
                    .replace(&old_num, &new_index.to_string());
            }
        }
    }

    pub fn to_byml_value(&self) -> Value {
        let mut model_unit = BTreeMap::new();
        model_unit.insert("Fmdb".to_string(), Value::String(self.model_fmdb.clone()));

        let mut root = BTreeMap::new();
        root.insert("Category".to_string(), Value::String(self.category.clone()));
        root.insert(
            "EditorIconName".to_string(),
            Value::String(self.editor_icon_name.clone()),
        );

        root.insert(
            "FileName".to_string(),
            Value::String(self.file_name.clone()),
        );

        root.insert(
            "IsVisibleInEditor".to_string(),
            Value::Bool(self.is_visible_in_editor),
        );

        root.insert("ModelUnit".to_string(), Value::Dict(model_unit));
        root.insert("PartsIndex".to_string(), Value::I32(self.parts_index));

        Value::Dict(root)
    }
}

#[derive(Debug)]
pub struct CustomFacelineMod {
    pub mod_index: usize,
    pub rstbl_entries: Vec<PartsProductFaceline>,
    pub pack_parts: BTreeMap<i32, FacelineParts>,
    pub custom_order_indices: Vec<i32>,
    pub custom_icon: Option<Bntx>,
    pub models: BTreeMap<String, Bfres>,
    pub pack_template: BTreeMap<String, Value>,
}

impl CustomFacelineMod {
    pub fn load(mod_index: usize, mod_dir: &PathBuf) -> Result<Option<Self>> {
        let mut rstbl_entries = Vec::new();
        let mut pack_parts = BTreeMap::new();
        let mut custom_order_indices = Vec::new();
        let mut custom_icon = None;
        let mut models = BTreeMap::new();
        let mut pack_template = BTreeMap::new();

        let mut has_custom_content = false;

        let rstbl_path = Path::new("romfs").join(PATH_RSTBL_BYML);
        if mod_dir.join(&rstbl_path).exists() {
            let bytes = read_and_decompress(mod_dir, rstbl_path.to_str().unwrap())?;
            let (doc, entries) = parse_byml_array(&bytes)?;

            pack_template = find_template_entry(&doc)?;

            for entry in &entries {
                if let Some(faceline) = PartsProductFaceline::from_byml_entry(entry)? {
                    rstbl_entries.push(faceline);

                    has_custom_content = true;
                }
            }
        }

        let pack_path = Path::new("romfs").join(PATH_MII_PARTS_PACK);
        if mod_dir.join(&pack_path).exists() {
            let bytes = read_and_decompress(mod_dir, pack_path.to_str().unwrap())?;
            let sarc = sarc_parse(bytes)?;
            for (file_in_sarc, data) in sarc_entries_owned(&sarc) {
                if file_in_sarc.contains("Faceline") && file_in_sarc.ends_with(".bgyml") {
                    if let Ok(part) = FacelineParts::from_bytes(&data) {
                        if part.parts_index > VANILLA_INDICES {
                            pack_parts.insert(part.parts_index, part);
                            has_custom_content = true;
                        }
                    }
                }
            }
        }

        let order_path = Path::new("romfs").join(PATH_PARTS_ORDER);
        if mod_dir.join(&order_path).exists() {
            let bytes = read_and_decompress(mod_dir, order_path.to_str().unwrap())?;
            let indices = parse_parts_order_array(&bytes)?;
            for idx in indices {
                if idx > VANILLA_INDICES {
                    custom_order_indices.push(idx);
                    has_custom_content = true;
                }
            }
        }

        let icon_pack_path = Path::new("romfs/Tex/Pack/MiiEditorIcon.bntx.zs");
        if mod_dir.join(icon_pack_path).exists() {
            let bytes = read_and_decompress(mod_dir, "romfs/Tex/Pack/MiiEditorIcon.bntx.zs")?;
            if let Ok(bntx) = Bntx::parse(&bytes) {
                custom_icon = Some(bntx);
            }
        }

        for entry in walkdir::WalkDir::new(mod_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }

            let rel_path = entry.path().strip_prefix(mod_dir)?;
            let rel_str = rel_path.to_str().unwrap_or("");
            let clean = clean_name(rel_str);

            let normal_path = clean.strip_prefix("romfs/").unwrap_or(clean);

            if normal_path.starts_with("Model/") && normal_path.ends_with(".bfres") {
                if let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) {
                    let clean_stem = stem.strip_suffix(".bfres").unwrap_or(stem);
                    let num_str: String =
                        clean_stem.chars().filter(|c| c.is_ascii_digit()).collect();

                    if let Ok(parts_idx) = num_str.parse::<i32>() {
                        if parts_idx > VANILLA_INDICES {
                            let raw_bytes = read_and_decompress(mod_dir, rel_str)?;
                            let bfres = bfres_parse(&raw_bytes)?;
                            models.insert(clean_stem.to_string(), bfres);
                            has_custom_content = true;
                        }
                    }
                }
            }
        }

        if !has_custom_content {
            return Ok(None);
        }

        Ok(Some(Self {
            mod_index,
            rstbl_entries,
            pack_parts,
            custom_order_indices,
            custom_icon,
            models,
            pack_template,
        }))
    }
}

pub struct FacelineMergeEngine {
    pub mods: Vec<CustomFacelineMod>,
    pub allocation_map: BTreeMap<(usize, i32), i32>,
    pub claimed_indices: BTreeMap<i32, usize>,
    pub next_free_index: i32,
}

impl FacelineMergeEngine {
    pub fn new(mods: Vec<CustomFacelineMod>) -> Result<Self> {
        Ok(Self {
            mods,
            allocation_map: BTreeMap::new(),
            claimed_indices: BTreeMap::new(),
            next_free_index: VANILLA_INDICES + 1,
        })
    }

    pub fn compute_allocations(&mut self) -> Result<()> {
        for custom_mod in &self.mods {
            let mod_idx = custom_mod.mod_index;

            let mut unique_mod_claims = BTreeSet::new();
            for entry in &custom_mod.rstbl_entries {
                unique_mod_claims.insert(entry.parts_index);
            }

            for &idx in &custom_mod.custom_order_indices {
                if idx > VANILLA_INDICES {
                    unique_mod_claims.insert(idx);
                }
            }

            for old_idx in unique_mod_claims {
                while self.next_free_index <= old_idx
                    || self.claimed_indices.contains_key(&self.next_free_index)
                {
                    self.next_free_index += 1;
                }

                if let Some(&owner) = self.claimed_indices.get(&old_idx) {
                    let assigned = self.next_free_index;
                    self.next_free_index += 1;

                    println!(
                        "[Collision] Mod {mod_idx} Faceline{old_idx} reassigned to Faceline{assigned} (Conflicted with Mod {owner})"
                    );

                    self.allocation_map.insert((mod_idx, old_idx), assigned);
                } else {
                    self.claimed_indices.insert(old_idx, mod_idx);
                    self.allocation_map.insert((mod_idx, old_idx), old_idx);
                }
            }
        }
        Ok(())
    }

    pub fn execute_remap(&mut self) -> Result<()> {
        for custom_mod in &mut self.mods {
            let mod_idx = custom_mod.mod_index;

            for entry in &mut custom_mod.rstbl_entries {
                let old_idx = entry.parts_index;
                if let Some(&new_idx) = self.allocation_map.get(&(mod_idx, old_idx)) {
                    entry.remap_to(new_idx);
                }
            }

            let mut updated_pack_parts = BTreeMap::new();
            for (old_idx, mut part) in std::mem::take(&mut custom_mod.pack_parts) {
                if let Some(&new_idx) = self.allocation_map.get(&(mod_idx, old_idx)) {
                    part.remap_to(old_idx, new_idx);
                    updated_pack_parts.insert(new_idx, part);
                } else {
                    updated_pack_parts.insert(old_idx, part);
                }
            }
            custom_mod.pack_parts = updated_pack_parts;

            for idx in &mut custom_mod.custom_order_indices {
                if *idx > VANILLA_INDICES {
                    if let Some(&new_idx) = self.allocation_map.get(&(mod_idx, *idx)) {
                        *idx = new_idx;
                    }
                }
            }

            let mut updated_models = BTreeMap::new();
            for (old_name, mut bfres) in std::mem::take(&mut custom_mod.models) {
                let num_str: String = old_name.chars().filter(|c| c.is_ascii_digit()).collect();
                if let Ok(old_parts_idx) = num_str.parse::<i32>() {
                    if let Some(&new_parts_idx) = self.allocation_map.get(&(mod_idx, old_parts_idx))
                    {
                        let new_name = format!("MiiHead{:02}", new_parts_idx);
                        println!("updated bfres model name to {new_name}");
                        bfres.name = new_name.clone();

                        updated_models.insert(new_name, bfres);
                        continue;
                    }
                }
                updated_models.insert(old_name, bfres);
            }

            custom_mod.models = updated_models;

            if let Some(ref mut bntx) = custom_mod.custom_icon {
                for tex in &mut bntx.textures {
                    let num_str: String = tex.name.chars().filter(|c| c.is_ascii_digit()).collect();
                    if let Ok(old_idx) = num_str.parse::<i32>() {
                        if let Some(&new_idx) = self.allocation_map.get(&(mod_idx, old_idx)) {
                            if new_idx != old_idx {
                                tex.name =
                                    tex.name.replace(&old_idx.to_string(), &new_idx.to_string());
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn parse_byml_array(bytes: &[u8]) -> Result<(Byml, Vec<Value>)> {
    let doc = byml_parse(bytes)?;

    let arr_ref = byml_root_array(&doc)
        .map_err(|_| anyhow::anyhow!("Failed to read BYML root as an array"))?;

    let arr = arr_ref.clone();

    Ok((doc, arr))
}

fn parse_parts_order_array(bytes: &[u8]) -> Result<Vec<i32>> {
    let doc = byml_parse(bytes)?;
    let root = byml_root_dict(&doc)?;

    let order_val = root
        .get("Order")
        .context("PartsOrder layout configuration is missing the 'Order' key")?;

    match order_val {
        Value::Array(arr) => arr
            .iter()
            .map(|v| match v {
                Value::U32(n) => Ok(*n as i32),
                Value::I32(n) => Ok(*n),
                _ => bail!("PartsOrder contains an unexpected non-integer type element"),
            })
            .collect(),

        _ => bail!("PartsOrder 'Order' key is structurally not a valid array matrix"),
    }
}
