use crate::category::{CategoryDef, PartsEntry};
use crate::merge::parse_parts_order_array;
use crate::util::*;
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tomolib::formats::bfres::Bfres;
use tomolib::formats::bntx::Bntx;

#[derive(Debug, Clone)]
pub struct CategoryModData {
    pub rstbl_entries: Vec<PartsEntry>,
    pub pack_parts: BTreeMap<i32, PartsEntry>,
}

#[derive(Debug)]
pub struct CustomModNamespace {
    pub mod_index: usize,
    pub categories: BTreeMap<String, CategoryModData>,
    pub custom_order_indices: Vec<(String, i32)>,
    pub custom_icon: Option<Bntx>,
    pub models: BTreeMap<String, Bfres>,
}

impl CustomModNamespace {
    pub fn load(
        mod_index: usize,
        mod_dir: &PathBuf,
        categories: &[Box<dyn CategoryDef>],
    ) -> Result<Option<Self>> {
        let mut cat_data_map = BTreeMap::new();
        let mut custom_order_indices = Vec::new();
        let mut custom_icon = None;
        let mut models = BTreeMap::new();
        let mut has_custom_content = false;

        for cat in categories {
            cat_data_map.insert(
                cat.category_name().to_string(),
                CategoryModData {
                    rstbl_entries: Vec::new(),
                    pack_parts: BTreeMap::new(),
                },
            );
        }

        let rstbl_path = Path::new("romfs").join(PATH_RSTBL_BYML);
        if mod_dir.join(&rstbl_path).exists() {
            let bytes = read_and_decompress(mod_dir, rstbl_path.to_str().unwrap())?;
            let doc = byml_parse(&bytes)?;
            let arr = byml_root_array(&doc)?;

            for val in arr {
                if let tomolib::formats::byml::Value::Dict(map) = val {
                    for cat in categories {
                        if let Some(entry) = crate::category::PartsEntry::from_byml_map(
                            map,
                            cat.internal_category_name(),
                            cat.vanilla_max_parts_index(),
                        )? {
                            cat_data_map
                                .get_mut(cat.category_name())
                                .unwrap()
                                .rstbl_entries
                                .push(entry);
                            has_custom_content = true;
                            break;
                        }
                    }
                }
            }
        }

        let pack_path = Path::new("romfs").join(PATH_MII_PARTS_PACK);
        if mod_dir.join(&pack_path).exists() {
            let bytes = read_and_decompress(mod_dir, pack_path.to_str().unwrap())?;
            let sarc = sarc_parse(bytes)?;
            for (file_in_sarc, data) in sarc_entries_owned(&sarc) {
                if file_in_sarc.ends_with(".bgyml") {
                    let doc = byml_parse(&data)?;
                    let root_map = byml_root_dict(&doc)?;

                    for cat in categories {
                        if let Some(entry) = crate::category::PartsEntry::from_byml_map(
                            root_map,
                            cat.internal_category_name(),
                            cat.vanilla_max_parts_index(),
                        )? {
                            cat_data_map
                                .get_mut(cat.category_name())
                                .unwrap()
                                .pack_parts
                                .insert(entry.parts_index, entry);
                            has_custom_content = true;
                            break;
                        }
                    }
                }
            }
        }

        for cat in categories {
            let order_path = Path::new("romfs").join(cat.path_parts_order());

            if mod_dir.join(&order_path).exists() {
                let bytes = read_and_decompress(mod_dir, order_path.to_str().unwrap())?;
                let indices = parse_parts_order_array(&bytes)?;

                for idx in indices {
                    if idx > cat.vanilla_max_parts_index() {
                        custom_order_indices.push((cat.category_name().to_string(), idx));
                        has_custom_content = true;
                    }
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
                        let is_modded = categories
                            .iter()
                            .any(|cat| parts_idx > cat.vanilla_max_parts_index());
                        if is_modded {
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
            categories: cat_data_map,
            custom_order_indices,
            custom_icon,
            models,
        }))
    }
}
