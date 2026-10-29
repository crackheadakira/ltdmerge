use crate::category::{AssetKind, CategoryDef, PartsEntry, str_field};
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
    pub order_doc: Option<&'a mut Byml>,
    pub base_bntx: Option<&'a mut Bntx>,
    pub editor_bntx: &'a mut Bntx,
    pub linked: &'a mut BTreeMap<&'static str, Byml>,
}

pub fn run(
    base: &Path,
    out_root: &Path,
    manifest: &AddManifest,
    registry: &CategoryRegistry,
) -> Result<()> {
    if manifest.assets.is_empty() {
        return Ok(());
    }

    add_multi(base, &out_root.join("romfs"), &manifest.assets, registry)
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
    let next_index = std::cmp::max(next_index, cat.vanilla_max_parts_index() as u32)
        .checked_add(1)
        .filter(|index| i32::try_from(*index).is_ok())
        .context("category has exhausted the signed PartsIndex range")?;

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

    let template_name = entry
        .template
        .clone()
        .unwrap_or_else(|| cat.template_part().to_string());

    let template = template_row(state.rstbl_doc, cat, &template_name)?;
    let template_icon = str_field(&template, "EditorIconName");
    let template_mask = Some(str_field(&template, "EditorMaskIconName"))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| template_icon.clone());

    if let Some(ref local_png_path) = entry.editor_icon_name {
        let icon = cat.editor_icon_name(next_index);
        println!("Injecting editor icon: '{icon}'");
        inject_texture(state.editor_bntx, local_png_path, &icon, &template_icon)?;
        entry.editor_icon_name = Some(icon);
    } else {
        entry.editor_icon_name = Some(template_icon.clone());
    }

    if let Some(ref local_png_path) = entry.editor_mask_icon_name
        && let Some(mask) = cat.editor_mask_icon_name(next_index)
    {
        println!("Injecting editor mask icon: '{mask}'");
        inject_texture(state.editor_bntx, local_png_path, &mask, &template_mask)?;
        entry.editor_mask_icon_name = Some(mask);
    }

    let no_lip_png = entry.no_lip_texture_name.clone();
    entry = entry.with_category_defaults(next_index, cat, &template)?;

    let mut romfs_files = BTreeMap::new();
    if is_bfres(&primary) {
        entry.model_unit.compile_and_finalize(
            &target_internal_token,
            cat.model_folder(),
            &target_internal_token,
            &mut romfs_files,
        )?;

        if let Some(ref mut hat_unit) = entry.model_unit_for_hat {
            let hat_token = format!("{}_Hat", target_internal_token);
            hat_unit.compile_and_finalize(
                &hat_token,
                cat.model_folder(),
                &target_internal_token,
                &mut romfs_files,
            )?;
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
        let Some(parts_bntx) = &mut state.base_bntx else {
            bail!(
                "Category '{}' received a non-BFRES asset but does not support direct BNTX texture injection.",
                cat.category_name()
            );
        };

        let template_texture = str_field(&template, "TextureName");
        println!("Injecting raw asset texture into MiiParts.bntx: '{target_internal_token}'");

        inject_texture(
            parts_bntx,
            &primary,
            &target_internal_token,
            &template_texture,
        )?;

        entry.texture_name = Some(target_internal_token.clone());

        if let Some(png) = no_lip_png {
            let no_lip = format!("{target_internal_token}_NoLip");
            println!("Injecting no-lip texture into MiiParts.bntx: '{no_lip}'");
            inject_texture(parts_bntx, &png, &no_lip, &template_texture)?;

            entry.no_lip_texture_name = Some(no_lip);
        }
    }

    for table in cat.linked_tables() {
        let doc = state
            .linked
            .get_mut(table.table)
            .context("linked table was not loaded")?;

        table.insert(doc, &mut entry)?;
    }

    entry.validate()?;
    let root_array = byml_root_array_mut(state.rstbl_doc)?;
    root_array.push(entry.build_rstbl_value());
    root_array.sort_by(|a, b| row_id_of(a).cmp(row_id_of(b)));

    let inner_bytes = byml_to_bytes(&Byml {
        version: 7,
        endian: tomolib::formats::byml::Endian::Little,
        root: cat.build_pack_value(&entry),
    })?;

    state
        .pack_entries
        .insert(cat.pack_path(&part_filename), inner_bytes);

    if let Some(order_doc) = state.order_doc.as_deref_mut() {
        let order_arr = match byml_root_dict_mut(order_doc)?.get_mut("Order") {
            Some(Value::Array(a)) => a,
            _ => bail!("PartsOrder missing 'Order' Array"),
        };

        order_arr.push(Value::U32(next_index));
    }

    Ok(written_sizes)
}

pub fn add_multi(
    base: &Path,
    out: &Path,
    assets: &[PartsEntry],
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
        .filter_map(|spec| registry.get(&spec.category))
        .any(|cat| cat.asset_kind() == AssetKind::Texture);

    let mut base_bntx = if batch_has_textures {
        let parts_bntx_rel = "Tex/Pack/MiiParts.bntx.zs";
        let raw_bntx_bytes = read_and_decompress(base, parts_bntx_rel)?;
        Some(Bntx::parse(&raw_bntx_bytes)?)
    } else {
        None
    };

    let mut linked = BTreeMap::new();

    for cat in assets
        .iter()
        .filter_map(|spec| registry.get(&spec.category))
    {
        for table in cat.linked_tables() {
            if !linked.contains_key(table.table) {
                linked.insert(
                    table.table,
                    byml_parse(&read_and_decompress(base, table.table)?)?,
                );
            }
        }
    }

    for (i, spec) in assets.iter().enumerate() {
        let cat = registry.get(&spec.category).expect("pre-verified");
        println!(
            "[{}/{}] Processing '{}' asset",
            i + 1,
            assets.len(),
            spec.category
        );

        let order_path = cat.path_parts_order();
        if let Some(path) = &order_path
            && !open_orders.contains_key(path)
        {
            open_orders.insert(path.clone(), byml_parse(&read_and_decompress(base, path)?)?);
        }

        let order_doc = order_path.and_then(|path| open_orders.get_mut(&path));

        let mut state = MutableState {
            rstbl_doc: &mut rstbl_doc,
            pack_entries: &mut pack_entries,
            rsizetable: &mut rsizetable,
            order_doc,
            base_bntx: base_bntx.as_mut(),
            editor_bntx: &mut editor_bntx,
            linked: &mut linked,
        };

        let local_sizes = mutate_asset_structures(spec.clone(), out, cat, &mut state)?;
        global_written_sizes.extend(local_sizes);
    }

    if let Some(base_bntx) = base_bntx {
        let bntx_size = compress_and_write(out, "Tex/Pack/MiiParts.bntx.zs", &base_bntx.write()?)?;
        global_written_sizes.insert(
            clean_name("romfs/Tex/Pack/MiiParts.bntx").to_string(),
            bntx_size,
        );
    }

    for (path, doc) in &linked {
        let size = compress_and_write(out, path, &byml_to_bytes(doc)?)?;

        patch_rstbl(&mut rsizetable, path, size)?;
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
    if let Value::Dict(m) = val
        && let Some(Value::String(s)) = m.get("__RowId")
    {
        return s.as_str();
    }

    ""
}

fn collect_category_indices(doc: &Byml, target_hash: u32) -> Result<Vec<u32>> {
    byml_root_array(doc)?
        .iter()
        .filter_map(|entry| {
            if let Value::Dict(m) = entry
                && let Some(Value::U32(parts_type)) = m.get("PartsType")
                && *parts_type == target_hash
            {
                match m.get("PartsIndex") {
                    Some(Value::I32(idx)) if *idx >= 0 => return Some(Ok(*idx as u32)),
                    Some(Value::U32(idx)) => return Some(Ok(*idx)),
                    _ => {}
                }
            }

            None
        })
        .collect()
}

fn template_row(
    doc: &Byml,
    cat: &dyn CategoryDef,
    template: &str,
) -> Result<BTreeMap<String, Value>> {
    let row_id = format!("Work/Mii/Parts/{template}.mii__Parts.gyml");

    let row = byml_root_array(doc)?
        .iter()
        .find_map(|row| match row {
            Value::Dict(map) if row_id_of(row) == row_id => Some(map.clone()),
            _ => None,
        })
        .with_context(|| format!("template part '{template}' is not in the base MiiParts table"))?;

    anyhow::ensure!(
        str_field(&row, "Category") == cat.internal_category_name(),
        "template part '{template}' is not a {} part",
        cat.category_name()
    );

    Ok(row)
}

fn inject_texture(bntx: &mut Bntx, png: &str, name: &str, template: &str) -> Result<()> {
    let png_bytes = std::fs::read(png).with_context(|| format!("reading PNG '{png}'"))?;
    let img = image::png_to_rgba(&png_bytes)?;

    let template = bntx
        .textures
        .iter()
        .find(|tex| tex.name == template)
        .with_context(|| format!("no template texture '{template}' for '{name}'"))?
        .clone();

    let image_data = encode_mips_swizzled(&img, &template)
        .with_context(|| format!("encoding '{png}' like '{}'", template.name))?;

    bntx.textures.retain(|t| t.name != name);
    bntx.textures.push(Texture {
        name: name.to_string(),
        info: template.info,
        mip_offsets: template.mip_offsets,
        user_data: vec![],
        image_data,
    });

    Ok(())
}
