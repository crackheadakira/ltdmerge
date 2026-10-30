use crate::category::{CategoryDef, PartsEntry};
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
                        if let Some(entry) =
                            crate::category::PartsEntry::from_byml_map(map, cat.as_ref())?
                        {
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
                        if let Some(entry) =
                            crate::category::PartsEntry::from_byml_map(root_map, cat.as_ref())?
                        {
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

        let icon_pack_path = Path::new("romfs/Tex/Pack/MiiEditorIcon.bntx.zs");
        if mod_dir.join(icon_pack_path).exists() {
            let bytes = read_and_decompress(mod_dir, "romfs/Tex/Pack/MiiEditorIcon.bntx.zs")?;
            custom_icon = Some(Bntx::parse(&bytes)?);
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

            if normal_path.starts_with("Model/")
                && normal_path.ends_with(".bfres")
                && let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str())
            {
                let clean_stem = stem.strip_suffix(".bfres").unwrap_or(stem);
                let is_owned = categories.iter().any(|cat| {
                    let data = &cat_data_map[cat.category_name()];

                    data.rstbl_entries
                        .iter()
                        .map(|e| e.parts_index)
                        .chain(data.pack_parts.keys().copied())
                        .any(|index| {
                            let name = cat.internal_model_name(index as u32);
                            clean_stem == name
                                || clean_stem
                                    .strip_prefix(&name)
                                    .is_some_and(|suffix| suffix.starts_with('_'))
                        })
                });

                if is_owned {
                    let raw_bytes = read_and_decompress(mod_dir, rel_str)?;
                    let bfres = bfres_parse(&raw_bytes)?;
                    models.insert(clean_stem.to_string(), bfres);
                    has_custom_content = true;
                }
            }
        }

        if !has_custom_content {
            return Ok(None);
        }

        Ok(Some(Self {
            mod_index,
            categories: cat_data_map,
            custom_icon,
            models,
        }))
    }
}
