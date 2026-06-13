use crate::category::{CategoryDef, i32_field};
use crate::engine::MiiMergeEngine;
use crate::rstbl::alloc_size;
use crate::types::CustomModNamespace;
use crate::util::*;
use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tomolib::formats::bntx::Bntx;
use tomolib::formats::byml::{Byml, Endian, Value};
use tomolib::formats::rstbl::Rstbl;

pub fn run(
    mod_dirs: Vec<PathBuf>,
    out_dir: PathBuf,
    categories: Vec<Box<dyn CategoryDef>>,
) -> Result<()> {
    if mod_dirs.len() < 2 {
        bail!("merge requires at least two input mod directories");
    }

    println!("Merging {} mods -> {}", mod_dirs.len(), out_dir.display());

    let mut custom_mods = Vec::new();
    for (idx, dir) in mod_dirs.iter().enumerate() {
        if let Some(loaded_mod) = CustomModNamespace::load(idx, dir, &categories)? {
            custom_mods.push(loaded_mod);
        }
    }

    let mut engine = MiiMergeEngine::new(custom_mods, categories);
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

        let mut entries_to_purge: BTreeMap<String, BTreeSet<i32>> = BTreeMap::new();
        for ((cat_name, _, old_idx), new_idx) in &engine.allocation_map {
            let set = entries_to_purge.entry(cat_name.clone()).or_default();
            set.insert(*old_idx);
            set.insert(*new_idx);
        }

        arr.retain(|val| {
            if let Value::Dict(map) = val {
                if let Some(Value::String(cat_name)) = map.get("Category") {
                    if let Some(purge_set) = entries_to_purge.get(cat_name) {
                        if let Some(idx) = i32_field(map, "PartsIndex") {
                            if purge_set.contains(&idx) {
                                return false;
                            }
                        }
                    }
                }
            }
            true
        });

        for custom_mod in &engine.mods {
            for cat in &engine.categories {
                if let Some(data) = custom_mod.categories.get(cat.category_name()) {
                    for entry in &data.rstbl_entries {
                        arr.push(cat.build_rstbl_value(entry));
                    }
                }
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
        for ((cat_name, _, old_idx), new_idx) in &engine.allocation_map {
            if let Some(cat) = engine
                .categories
                .iter()
                .find(|c| c.category_name() == cat_name)
            {
                files_to_remove.insert(cat.pack_path(&cat.part_name(*old_idx as u32)));
                files_to_remove.insert(cat.pack_path(&cat.part_name(*new_idx as u32)));
            }
        }

        merged_entries.retain(|key, _| !files_to_remove.contains(key));

        for custom_mod in &engine.mods {
            for cat in &engine.categories {
                if let Some(data) = custom_mod.categories.get(cat.category_name()) {
                    for (parts_idx, entry) in &data.pack_parts {
                        let file_name = cat.part_name(*parts_idx as u32);
                        let sarc_internal_key = cat.pack_path(&file_name);

                        let doc_wrapper = Byml {
                            root: cat.build_pack_value(entry),
                            endian: Endian::Little,
                            version: 7,
                        };

                        merged_entries.insert(sarc_internal_key, byml_to_bytes(&doc_wrapper)?);
                    }
                }
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

    for cat in &engine.categories {
        let order_rel_path = Path::new("romfs").join(cat.path_parts_order());
        if let Some(base_mod_dir) = mod_dirs
            .iter()
            .rev()
            .find(|d| d.join(&order_rel_path).exists())
        {
            let raw_bytes = read_and_decompress(base_mod_dir, order_rel_path.to_str().unwrap())?;
            let mut doc = byml_parse(&raw_bytes)?;
            let mut unified_order = parse_parts_order_array(&raw_bytes)?;

            for custom_mod in &engine.mods {
                for (cat_name, custom_idx) in &custom_mod.custom_order_indices {
                    if cat_name == cat.category_name()
                        && !unified_order.contains(&(*custom_idx as i32))
                    {
                        unified_order.push(*custom_idx as i32);
                    }
                }
            }

            let array_values: Vec<Value> = unified_order.into_iter().map(Value::I32).collect();
            byml_root_dict_mut(&mut doc)?.insert("Order".to_string(), Value::Array(array_values));

            let ordered_bytes = byml_to_bytes(&doc)?;
            let size =
                compress_and_write(&out_dir, order_rel_path.to_str().unwrap(), &ordered_bytes)?;

            written_sizes.insert(clean_name(cat.path_parts_order()).to_string(), size);
        }
    }

    for custom_mod in &engine.mods {
        for (head_name, bfres) in &custom_mod.models {
            let out_rel_path = format!("romfs/Model/{head_name}.bfres.zs");
            let raw_bfres_payload = bfres.write().context("BFRES layout serialization failed")?;
            let size = compress_and_write(&out_dir, &out_rel_path, &raw_bfres_payload)?;
            written_sizes.insert(format!("Model/{head_name}.bfres"), size);
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
                        let is_vanilla = engine
                            .categories
                            .iter()
                            .any(|c| parts_idx <= c.vanilla_max_parts_index());
                        if is_vanilla {
                            let raw_bytes = read_and_decompress(dir, rel_str)?;
                            let size = compress_and_write(&out_dir, rel_str, &raw_bytes)?;
                            written_sizes.insert(clean.to_string(), size);
                        }
                    }
                }
            }
        }
    }

    let parts_bntx_rel = Path::new("romfs").join("Tex/Pack/MiiParts.bntx.zs");
    if let Some(base_mod_dir) = mod_dirs
        .iter()
        .rev()
        .find(|d| d.join(&parts_bntx_rel).exists())
    {
        println!("Merging global flat texture atlas sheet: MiiParts.bntx.zs");
        let raw_bytes = read_and_decompress(base_mod_dir, parts_bntx_rel.to_str().unwrap())?;
        let mut base_bntx =
            Bntx::parse(&raw_bytes).context("Failed to parse base MiiParts.bntx")?;

        let mut textures_to_remove = BTreeSet::new();
        for ((cat_name, _, old_idx), new_idx) in &engine.allocation_map {
            if let Some(cat) = engine
                .categories
                .iter()
                .find(|c| c.category_name() == cat_name)
            {
                textures_to_remove.insert(cat.internal_model_name(*old_idx as u32));
                textures_to_remove.insert(cat.internal_model_name(*new_idx as u32));
            }
        }

        base_bntx
            .textures
            .retain(|tex| !textures_to_remove.contains(&tex.name));

        for custom_mod in &engine.mods {
            let mod_idx = custom_mod.mod_index;
            let target_path = mod_dirs[mod_idx].join(&parts_bntx_rel);

            if target_path.exists() {
                let loaded_bytes =
                    read_and_decompress(&mod_dirs[mod_idx], parts_bntx_rel.to_str().unwrap())?;
                if let Ok(mod_bntx) = Bntx::parse(&loaded_bytes) {
                    for mut tex in mod_bntx.textures {
                        for cat in &engine.categories {
                            if cat.matches_texture_name(&tex.name) {
                                let num_str: String =
                                    tex.name.chars().filter(|c| c.is_ascii_digit()).collect();
                                if let Ok(old_idx) = num_str.parse::<i32>() {
                                    if let Some(&new_idx) = engine.allocation_map.get(&(
                                        cat.category_name().to_string(),
                                        mod_idx,
                                        old_idx,
                                    )) {
                                        let old_token = cat.internal_model_name(old_idx as u32);
                                        let new_token = cat.internal_model_name(new_idx as u32);

                                        tex.name = tex.name.replace(&old_token, &new_token);
                                    }
                                }
                                break;
                            }
                        }
                        base_bntx.textures.push(tex);
                    }
                }
            }
        }

        let merged_bntx_bytes = base_bntx
            .write()
            .context("BNTX layout generation compression failed")?;

        let size = compress_and_write(
            &out_dir,
            parts_bntx_rel.to_str().unwrap(),
            &merged_bntx_bytes,
        )?;

        written_sizes.insert(clean_name("romfs/Tex/Pack/MiiParts.bntx").to_string(), size);
    }

    let icon_rel_path = Path::new("romfs").join(PATH_MII_EDITOR_ICON);
    if let Some(base_mod) = engine.mods.iter().find(|m| m.custom_icon.is_some()) {
        let mut base_bntx = base_mod.custom_icon.clone().unwrap();

        for custom_mod in &engine.mods {
            if custom_mod.mod_index == base_mod.mod_index {
                continue;
            }

            if let Some(ref mod_bntx) = custom_mod.custom_icon {
                for tex in &mod_bntx.textures {
                    let num_str: String = tex.name.chars().filter(|c| c.is_ascii_digit()).collect();
                    if let Ok(idx) = num_str.parse::<i32>() {
                        let is_modded = engine
                            .categories
                            .iter()
                            .any(|cat| idx > cat.vanilla_max_parts_index());

                        if is_modded {
                            base_bntx.textures.push(tex.clone());
                        }
                    }
                }
            }
        }

        let merged_bntx_bytes = base_bntx
            .write()
            .context("BNTX execution compression failed")?;
        let size = compress_and_write(
            &out_dir,
            icon_rel_path.to_str().unwrap(),
            &merged_bntx_bytes,
        )?;

        written_sizes.insert(clean_name(PATH_MII_EDITOR_ICON).to_string(), size);
    }

    let rsizetable_rel = Path::new("romfs").join(PATH_RSIZETABLE);
    if let Some(base_mod_dir) = mod_dirs
        .iter()
        .rev()
        .find(|d| d.join(&rsizetable_rel).exists())
    {
        println!("Patching and re-compiling rsizetable...");
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

pub fn parse_parts_order_array(bytes: &[u8]) -> Result<Vec<i32>> {
    let doc = byml_parse(bytes)?;
    let root = byml_root_dict(&doc)?;
    let order_val = root
        .get("Order")
        .context("PartsOrder missing 'Order' key")?;

    match order_val {
        Value::Array(arr) => arr
            .iter()
            .map(|v| match v {
                Value::U32(n) => Ok(*n as i32),
                Value::I32(n) => Ok(*n),
                _ => bail!("PartsOrder contains an unexpected element type"),
            })
            .collect(),

        _ => bail!("PartsOrder 'Order' key is structurally not a valid array matrix"),
    }
}
