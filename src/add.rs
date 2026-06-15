use crate::category::CategoryDef;
use crate::manifest::{AddManifest, AssetSpec};
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
        let spec = &manifest.assets[0];
        let cat = registry
            .get(&spec.category)
            .expect("category name was pre-validated in main");

        println!("[1/1] Adding '{}' asset", spec.category);
        add_single(base, &out, spec, cat)
            .with_context(|| format!("category '{}'", spec.category))?;
    }

    Ok(())
}

pub fn mutate_asset_structures(
    spec: &AssetSpec,
    out: &Path,
    cat: &dyn CategoryDef,
    state: &mut MutableState,
) -> Result<BTreeMap<String, usize>> {
    let written_sizes = BTreeMap::new();

    let mut params = cat
        .parse_asset_params(&spec.params)
        .with_context(|| format!("parsing params for category '{}'", spec.category))?;

    let primary = params.primary_source().to_string();

    let existing_indices = collect_category_indices(state.rstbl_doc, cat.parts_type_hash())?;
    let next_index = existing_indices.iter().copied().max().unwrap_or(0);
    let next_index = std::cmp::max(next_index, cat.vanilla_max_parts_index() as u32) + 1;

    let part_filename = cat.part_name(next_index);
    let target_internal_token = cat.internal_model_name(next_index);
    println!("Assigning index {next_index} → '{part_filename}'");

    let compiled = cat.compile_asset(next_index, &target_internal_token, params.as_mut())?;

    let template = find_template_entry(state.rstbl_doc, cat.category_name())?;
    let editor_icon_name = match &spec.icon {
        Some(_) => cat.editor_icon_name(next_index),
        None => cat.vanilla_icon_fallback().to_string(),
    };

    let mut entry = cat.new_entry(
        next_index,
        Some(editor_icon_name.clone()),
        params.as_ref(),
        &template,
    )?;

    if compiled.romfs_files.is_empty() && compiled.pack_files.is_empty() {
        if !is_bfres(&primary) {
            if let Some(ref mut bntx_archive) = state.base_bntx {
                let png_bytes = std::fs::read(&primary)?;
                let img = image::png_to_rgba(&png_bytes)?;

                let template_tex = bntx_archive
                    .textures
                    .iter()
                    .find(|tex| cat.matches_texture_name(&tex.name))
                    .context("MiiParts.bntx template match failed")?
                    .clone();

                bntx_archive.textures.push(Texture {
                    name: target_internal_token.clone(),
                    info: template_tex.info.clone(),
                    mip_offsets: template_tex.mip_offsets.clone(),
                    user_data: vec![],
                    image_data: encode_mips_swizzled(&img, &template_tex)?,
                });
            }
        }
    } else {
        for (lookup_key, raw_bytes) in compiled.romfs_files {
            let rel_path = format!("{lookup_key}.zs");
            let size = compress_and_write(out, &rel_path, &raw_bytes)?;

            let mut path_entries = state.rsizetable.path_entries().to_vec();
            path_entries.push(PathEntry {
                size: alloc_size(&lookup_key, size)?,
                name: lookup_key,
            });
            path_entries.sort_by_key(|f| f.name.clone());
            state.rsizetable.set_path_entries(path_entries);
        }

        for (archive_internal_path, raw_bytes) in compiled.pack_files {
            state.pack_entries.insert(archive_internal_path, raw_bytes);
        }
    }

    let template_index = match template.get("PartsIndex") {
        Some(Value::I32(n)) => *n as u32,
        Some(Value::U32(n)) => *n,
        _ => 15,
    };
    let old_token = cat.internal_model_name(template_index);

    entry.remap_to(
        next_index as i32,
        part_filename.clone(),
        cat.row_id(next_index),
        &old_token,
        &target_internal_token,
        cat,
    );

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
        let insert_pos = spec.order_index.unwrap_or(order_arr.len());
        order_arr.insert(insert_pos, Value::U32(next_index));
    }

    if let Some(ref icon_path) = spec.icon {
        inject_bntx_icon(icon_path, &editor_icon_name, cat, state.editor_bntx)?;
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
                cat.category_name()
            )
        })?
        .clone();

    println!("Layout template: '{}'", template.name);

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
