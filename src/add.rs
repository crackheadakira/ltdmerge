use crate::category::{CategoryDef, PartsEntry};
use crate::manifest::AddManifest;
use crate::registry::CategoryRegistry;
use crate::rstbl::alloc_size;
use crate::util::*;
use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use std::collections::BTreeMap;
use std::path::Path;
use tomolib::formats::bntx::image::{self, encode_mips_swizzled};
use tomolib::formats::bntx::{Bntx, Texture};
use tomolib::formats::byml::{Byml, Value};
use tomolib::formats::rstbl::{PathEntry, Rstbl};

pub struct MutableState<'a> {
    pub rstbl_doc: &'a mut Byml,
    pub pack_entries: &'a mut IndexMap<String, Vec<u8>>,
    pub rsizetable: &'a mut Rstbl,
    pub order_doc: &'a mut Byml,
    pub base_bntx: Option<&'a mut Bntx>,
    pub editor_bntx: &'a mut Bntx,
}

pub fn run(
    base: &Path,
    out_root: &Path,
    manifest: &AddManifest,
    registry: &CategoryRegistry,
) -> Result<()> {
    let out = out_root.join("romfs");

    if manifest.assets.is_empty() {
        return Ok(());
    }

    if manifest.assets.len() > 1 {
        add_multi(base, &out, &manifest.assets, registry)?;
    } else {
        let entry = &manifest.assets[0];
        let cat = registry
            .get(&entry.category)
            .expect("category name was pre-validated in main");

        println!("[1/1] Adding '{}' asset", entry.category);
        add_single(base, &out, entry, cat)
            .with_context(|| format!("category '{}'", entry.category))?;
    }

    Ok(())
}

pub fn mutate_asset_structures(
    mut entry: PartsEntry,
    out: &Path,
    cat: &dyn CategoryDef,
    state: &mut MutableState,
) -> Result<BTreeMap<String, usize>> {
    let mut written_sizes = BTreeMap::new();

    let existing_indices = collect_category_indices(state.rstbl_doc, cat.parts_type_hash())?;
    let next_index = existing_indices.iter().copied().max().unwrap_or(0);
    let next_index = std::cmp::max(next_index, cat.vanilla_max_parts_index() as u32) + 1;

    let part_filename = cat.part_name(next_index);
    let target_internal_token = cat.internal_model_name(next_index);
    println!("Assigning index {next_index} -> '{part_filename}'");

    let primary = entry
        .model_unit
        .fmdb
        .clone()
        .or_else(|| entry.texture_name.clone())
        .context(
            "Manifest entry is missing both a model_unit.fmdb and a texture_name source path",
        )?;

    if let Some(ref local_png_path) = entry.editor_icon_name {
        let final_game_icon_token = cat.editor_icon_name(next_index);
        inject_bntx_icon(
            Path::new(local_png_path),
            &final_game_icon_token,
            cat,
            state.editor_bntx,
        )?;

        entry.editor_icon_name = Some(final_game_icon_token);
    } else {
        entry.editor_icon_name = Some(cat.vanilla_icon_fallback().to_string());
    }

    if let Some(ref local_png_path) = entry.editor_mask_icon_name {
        let final_game_icon_token = cat.editor_mask_icon_name(next_index);
        inject_bntx_icon(
            Path::new(local_png_path),
            &final_game_icon_token,
            cat,
            state.editor_bntx,
        )?;

        entry.editor_mask_icon_name = Some(final_game_icon_token);
    }

    entry = cat.apply_category_defaults(next_index, entry);

    let mut romfs_files = BTreeMap::new();
    if is_bfres(&primary) {
        entry.model_unit.compile_and_finalize(
            &target_internal_token,
            &target_internal_token,
            &mut romfs_files,
        )?;

        if let Some(ref mut hat_unit) = entry.model_unit_for_hat {
            let hat_token = format!("{}_Hat", target_internal_token);
            hat_unit.compile_and_finalize(&hat_token, &target_internal_token, &mut romfs_files)?;
        }

        for (lookup_key, raw_bytes) in romfs_files {
            let rel_path = format!("{lookup_key}.zs");
            let size = compress_and_write(out, &rel_path, &raw_bytes)?;

            let mut path_entries = state.rsizetable.path_entries().to_vec();
            path_entries.push(PathEntry {
                size: alloc_size(&lookup_key, size)?,
                name: lookup_key,
            });
            path_entries.sort_by_key(|f| f.name.clone());
            state.rsizetable.set_path_entries(path_entries);

            written_sizes.insert(format!("romfs/{rel_path}"), size);
        }
    } else {
        if let Some(ref mut bntx_archive) = state.base_bntx {
            println!("Injecting raw asset texture into MiiParts.bntx: '{target_internal_token}'");
            let png_bytes = std::fs::read(&primary).with_context(|| {
                format!("failed to read texture asset source file: {}", primary)
            })?;
            let img = image::png_to_rgba(&png_bytes)?;

            let template_tex = bntx_archive
                .textures
                .iter()
                .find(|tex| cat.matches_texture_name(&tex.name))
                .with_context(|| {
                    format!(
                        "MiiParts.bntx template match failed for category '{}'",
                        cat.category_name()
                    )
                })?
                .clone();

            bntx_archive.textures.push(Texture {
                name: target_internal_token.clone(),
                info: template_tex.info.clone(),
                mip_offsets: template_tex.mip_offsets.clone(),
                user_data: vec![],
                image_data: encode_mips_swizzled(&img, &template_tex)?,
            });

            entry.texture_name = Some(target_internal_token.clone());
        } else {
            bail!(
                "Category '{}' received a non-BFRES asset but does not support direct BNTX texture injection.",
                cat.category_name()
            );
        }
    }

    let root_array = byml_root_array_mut(state.rstbl_doc)?;
    root_array.push(entry.build_rstbl_value());
    root_array.sort_by(|a, b| row_id_of(a).cmp(row_id_of(b)));

    let inner_bytes = byml_to_bytes(&Byml {
        version: 7,
        endian: tomolib::formats::byml::Endian::Little,
        root: entry.build_pack_value(),
    })?;

    state
        .pack_entries
        .insert(cat.pack_path(&part_filename), inner_bytes);

    {
        let order_arr = match byml_root_dict_mut(state.order_doc)?.get_mut("Order") {
            Some(Value::Array(a)) => a,
            _ => bail!("PartsOrder missing 'Order' Array"),
        };

        order_arr.push(Value::U32(next_index));
    }

    Ok(written_sizes)
}

fn add_single(base: &Path, out: &Path, spec: &AssetSpec, cat: &dyn CategoryDef) -> Result<()> {
    let rstbl_bytes = read_and_decompress(base, PATH_RSTBL_BYML)?;
    let mut rstbl_doc = byml_parse(&rstbl_bytes)?;

    let pack_bytes = read_and_decompress(base, PATH_MII_PARTS_PACK)?;
    let pack_sarc = sarc_parse(pack_bytes)?;
    let mut pack_entries = sarc_entries_owned(&pack_sarc);

    let order_bytes = read_and_decompress(base, cat.path_parts_order())?;
    let mut order_doc = byml_parse(&order_bytes)?;

    let rsizetable_bytes = read_and_decompress(base, PATH_RSIZETABLE)?;
    let mut rsizetable = Rstbl::parse(&rsizetable_bytes)?;

    let icon_bytes = read_and_decompress(base, PATH_MII_EDITOR_ICON)?;
    let mut editor_bntx = Bntx::parse(&icon_bytes)?;

    let is_texture_asset =
        ["eye", "beard", "mouth"].contains(&spec.category.to_lowercase().as_str());

    let mut base_bntx = if is_texture_asset {
        let raw_bntx_bytes = read_and_decompress(base, "Tex/Pack/MiiParts.bntx.zs")?;
        Some(Bntx::parse(&raw_bntx_bytes)?)
    } else {
        None
    };

    let mut written_sizes = {
        let mut state = MutableState {
            rstbl_doc: &mut rstbl_doc,
            pack_entries: &mut pack_entries,
            rsizetable: &mut rsizetable,
            order_doc: &mut order_doc,
            base_bntx: base_bntx.as_mut(),
            editor_bntx: &mut editor_bntx,
        };
        mutate_asset_structures(spec, out, cat, &mut state)?
    };

    if let Some(base_bntx) = base_bntx {
        let bntx_size = compress_and_write(out, "Tex/Pack/MiiParts.bntx.zs", &base_bntx.write()?)?;
        written_sizes.insert(
            clean_name("romfs/Tex/Pack/MiiParts.bntx").to_string(),
            bntx_size,
        );
    }

    let icon_size = compress_and_write(out, PATH_MII_EDITOR_ICON, &editor_bntx.write()?)?;
    patch_rstbl(&mut rsizetable, PATH_MII_EDITOR_ICON, icon_size)?;

    let mii_pack_size = compress_and_write(
        out,
        PATH_MII_PARTS_PACK,
        &sarc_pack(&pack_entries, &pack_sarc)?,
    )?;

    let rstbl_size = compress_and_write(out, PATH_RSTBL_BYML, &byml_to_bytes(&rstbl_doc)?)?;
    let parts_order_size =
        compress_and_write(out, cat.path_parts_order(), &byml_to_bytes(&order_doc)?)?;

    patch_rstbl(&mut rsizetable, PATH_RSTBL_BYML, rstbl_size)?;
    patch_rstbl(&mut rsizetable, cat.path_parts_order(), parts_order_size)?;
    patch_rstbl(&mut rsizetable, PATH_MII_PARTS_PACK, mii_pack_size)?;

    for (clean_path, decompressed_size) in &written_sizes {
        patch_rstbl(&mut rsizetable, clean_path, *decompressed_size)?;
    }

    let mut rsizetable_buf = Vec::new();
    rsizetable.write(&mut rsizetable_buf)?;
    compress_and_write(out, PATH_RSIZETABLE, &rsizetable_buf)?;

    println!("Done.");
    Ok(())
}

pub fn add_multi(
    base: &Path,
    out: &Path,
    assets: &[AssetSpec],
    registry: &CategoryRegistry,
) -> Result<()> {
    let rstbl_bytes = read_and_decompress(base, PATH_RSTBL_BYML)?;
    let mut rstbl_doc = byml_parse(&rstbl_bytes)?;

    let pack_bytes = read_and_decompress(base, PATH_MII_PARTS_PACK)?;
    let pack_sarc = sarc_parse(pack_bytes)?;
    let mut pack_entries = sarc_entries_owned(&pack_sarc);

    let rsizetable_bytes = read_and_decompress(base, PATH_RSIZETABLE)?;
    let mut rsizetable = Rstbl::parse(&rsizetable_bytes)?;

    let mut open_orders = BTreeMap::new();
    let mut global_written_sizes = BTreeMap::new();

    let icon_bytes = read_and_decompress(base, PATH_MII_EDITOR_ICON)?;
    let mut editor_bntx = Bntx::parse(&icon_bytes)?;

    let batch_has_textures = assets
        .iter()
        .any(|spec| ["eye", "beard", "mouth"].contains(&spec.category.to_lowercase().as_str()));

    let mut base_bntx = if batch_has_textures {
        let parts_bntx_rel = "Tex/Pack/MiiParts.bntx.zs";
        let raw_bntx_bytes = read_and_decompress(base, parts_bntx_rel)?;
        Some(Bntx::parse(&raw_bntx_bytes)?)
    } else {
        None
    };

    for (i, spec) in assets.iter().enumerate() {
        let cat = registry.get(&spec.category).expect("pre-verified");
        println!(
            "[{}/{}] Processing '{}' asset",
            i + 1,
            assets.len(),
            spec.category
        );

        let order_path = cat.path_parts_order().to_string();
        if !open_orders.contains_key(&order_path) {
            let order_bytes = read_and_decompress(base, &order_path)?;
            open_orders.insert(order_path.clone(), byml_parse(&order_bytes)?);
        }
        let order_doc = open_orders.get_mut(&order_path).unwrap();

        let mut state = MutableState {
            rstbl_doc: &mut rstbl_doc,
            pack_entries: &mut pack_entries,
            rsizetable: &mut rsizetable,
            order_doc,
            base_bntx: base_bntx.as_mut(),
            editor_bntx: &mut editor_bntx,
        };

        let local_sizes = mutate_asset_structures(spec, out, cat, &mut state)?;
        global_written_sizes.extend(local_sizes);
    }

    if let Some(base_bntx) = base_bntx {
        let bntx_size = compress_and_write(out, "Tex/Pack/MiiParts.bntx.zs", &base_bntx.write()?)?;
        global_written_sizes.insert(
            clean_name("romfs/Tex/Pack/MiiParts.bntx").to_string(),
            bntx_size,
        );
    }

    let icon_size = compress_and_write(out, PATH_MII_EDITOR_ICON, &editor_bntx.write()?)?;
    patch_rstbl(&mut rsizetable, PATH_MII_EDITOR_ICON, icon_size)?;

    for (order_path, doc) in open_orders {
        let size = compress_and_write(out, &order_path, &byml_to_bytes(&doc)?)?;
        patch_rstbl(&mut rsizetable, &order_path, size)?;
    }

    let mii_pack_size = compress_and_write(
        out,
        PATH_MII_PARTS_PACK,
        &sarc_pack(&pack_entries, &pack_sarc)?,
    )?;
    let rstbl_size = compress_and_write(out, PATH_RSTBL_BYML, &byml_to_bytes(&rstbl_doc)?)?;

    patch_rstbl(&mut rsizetable, PATH_RSTBL_BYML, rstbl_size)?;
    patch_rstbl(&mut rsizetable, PATH_MII_PARTS_PACK, mii_pack_size)?;

    for (clean_path, decompressed_size) in &global_written_sizes {
        patch_rstbl(&mut rsizetable, clean_path, *decompressed_size)?;
    }

    let mut rsizetable_buf = Vec::new();
    rsizetable.write(&mut rsizetable_buf)?;
    compress_and_write(out, PATH_RSIZETABLE, &rsizetable_buf)?;

    println!("All assets successfully merged and saved.");
    Ok(())
}

fn is_bfres(path: &str) -> bool {
    path.ends_with(".bfres") || path.ends_with(".bfres.zs")
}

fn row_id_of(val: &Value) -> &str {
    if let Value::Dict(m) = val {
        if let Some(Value::String(s)) = m.get("__RowId") {
            return s.as_str();
        }
    }
    ""
}

fn collect_category_indices(doc: &Byml, target_hash: u32) -> Result<Vec<u32>> {
    byml_root_array(doc)?
        .iter()
        .filter_map(|entry| {
            if let Value::Dict(m) = entry {
                if let Some(Value::U32(parts_type)) = m.get("PartsType") {
                    if *parts_type == target_hash {
                        if let Some(Value::I32(idx)) = m.get("PartsIndex") {
                            if *idx >= 0 {
                                return Some(Ok(*idx as u32));
                            }
                        }
                    }
                }
            }
            None
        })
        .collect()
}

pub fn find_template_entry(doc: &Byml, target_category: &str) -> Result<BTreeMap<String, Value>> {
    byml_root_array(doc)?
        .iter()
        .filter_map(|entry| {
            if let Value::Dict(m) = entry {
                if let Some(Value::String(cat)) = m.get("Category") {
                    if cat == target_category {
                        if let Some(Value::I32(idx)) = m.get("PartsIndex") {
                            if *idx > 0 {
                                return Some((*idx, m.clone()));
                            }
                        }
                    }
                }
            }
            None
        })
        .max_by_key(|(idx, _)| *idx)
        .map(|(_, m)| m)
        .context(format!(
            "rstbl.byml has no existing '{target_category}' entries to template from",
        ))
}

fn inject_bntx_icon(
    icon_png: &Path,
    texture_name: &str,
    cat: &dyn CategoryDef,
    bntx: &mut Bntx,
) -> Result<()> {
    println!("Injecting editor icon: '{texture_name}'");
    let png_bytes = std::fs::read(icon_png)
        .with_context(|| format!("reading icon '{}'", icon_png.display()))?;
    let img = image::png_to_rgba(&png_bytes)?;

    bntx.textures.retain(|t| t.name != texture_name);

    let template = bntx
        .textures
        .iter()
        .find(|tex| cat.matches_icon_name(&tex.name))
        .with_context(|| {
            format!(
                "MiiEditorIcon.bntx has no vanilla textures matching category '{}'",
                cat.internal_category_name()
            )
        })?
        .clone();

    println!("Layout icon template: '{}'", template.name);

    let swizzled_data = encode_mips_swizzled(&img, &template)?;
    bntx.textures.push(Texture {
        name: texture_name.to_string(),
        info: template.info,
        mip_offsets: template.mip_offsets,
        user_data: vec![],
        image_data: swizzled_data,
    });

    Ok(())
}
