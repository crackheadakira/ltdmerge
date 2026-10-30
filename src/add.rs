use crate::category::{
    AssetKind, CategoryDef, HairPartType, OrderEncoding, PartsEntry, PartsModelUnitParam,
    attach_icon_name, i32_field, str_field,
};
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
    pub orders: &'a mut BTreeMap<String, Byml>,
    pub base_bntx: Option<&'a mut Bntx>,
    pub editor_bntx: &'a mut Bntx,
    pub linked: &'a mut BTreeMap<&'static str, Byml>,
    pub ids: &'a mut BTreeMap<String, String>,
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

    if let Some(id) = &entry.id {
        anyhow::ensure!(
            state
                .ids
                .insert(id.clone(), part_filename.clone())
                .is_none(),
            "asset id '{id}' is used twice"
        );
    }

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

    if cat.asset_kind() == AssetKind::Texture {
        let Some(parts_bntx) = &mut state.base_bntx else {
            bail!("MiiParts.bntx was not loaded for '{}'", cat.category_name());
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
    } else {
        anyhow::ensure!(
            is_bfres(&primary),
            "'{}' needs a .bfres in model_unit.fmdb",
            cat.category_name()
        );

        for (rel, bytes) in compile_models(&mut entry, cat, &primary, &target_internal_token)? {
            let stored = if rel.ends_with(".bfres") {
                format!("{rel}.zs")
            } else {
                rel.clone()
            };
            let size = compress_and_write(out, &stored, &bytes)?;

            let mut path_entries = state.rsizetable.path_entries().to_vec();

            path_entries.push(PathEntry {
                size: alloc_size(&rel, size)?,
                name: rel,
            });

            path_entries.sort_by_key(|f| f.name.clone());
            state.rsizetable.set_path_entries(path_entries);

            written_sizes.insert(format!("romfs/{stored}"), size);
        }
    }

    let attached = attach_hair_parts(&mut entry, state)?;

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

    for order in cat.parts_orders() {
        let doc = state
            .orders
            .get_mut(&order.path)
            .context("PartsOrder was not loaded")?;

        let Some(Value::Array(values)) = byml_root_dict_mut(doc)?.get_mut("Order") else {
            bail!("{} has no 'Order' array", order.path);
        };

        values.push(Value::U32(order.encoding.encode(next_index, None)));
        if order.encoding == OrderEncoding::HairBack {
            for hair_parts in &attached {
                values.push(Value::U32(
                    order.encoding.encode(next_index, Some(*hair_parts)),
                ));
            }
        }
    }

    Ok(written_sizes)
}

fn compile_models(
    entry: &mut PartsEntry,
    cat: &dyn CategoryDef,
    source: &str,
    token: &str,
) -> Result<Vec<(String, Vec<u8>)>> {
    let mut bfres = bfres_parse(&read_and_decompress(Path::new(""), source)?)?;
    let names = bfres.models.names.clone();
    anyhow::ensure!(!names.is_empty(), "'{source}' contains no models");

    let fmdb = |model: &str| {
        format!(
            "Work/Model/Mii/{}/{token}/output/{model}.fmdb",
            cat.model_folder()
        )
    };

    let pick = |unit: Option<&PartsModelUnitParam>,
                auto: &dyn Fn(&str) -> bool|
     -> Result<Option<String>> {
        match unit.and_then(|u| u.fmdb.as_deref()) {
            Some(name) => {
                anyhow::ensure!(
                    names.iter().any(|n| n == name),
                    "'{source}' has no model '{name}' (models: {})",
                    names.join(", ")
                );
                Ok(Some(name.to_string()))
            }
            None => Ok(names.iter().find(|n| auto(n)).cloned()),
        }
    };

    let mut files = Vec::new();
    let mut cloth: BTreeMap<String, String> = BTreeMap::new();
    let mut cloth_path = |phcl: Option<&str>, suffix: &str| -> Result<Option<String>> {
        let Some(phcl) = phcl.filter(|p| !p.is_empty()) else {
            return Ok(None);
        };

        if phcl.starts_with("Work/") {
            return Ok(Some(phcl.to_string()));
        }

        if let Some(path) = cloth.get(phcl) {
            return Ok(Some(path.clone()));
        }

        let name = format!("{token}{suffix}");
        let bytes = std::fs::read(phcl).with_context(|| format!("reading cloth '{phcl}'"))?;
        files.push((format!("Phive/Cloth/{name}.Nin_NX_NVN.bphcl"), bytes));

        let path = format!("Work/Phive/Cloth/{name}.phcl");
        cloth.insert(phcl.to_string(), path.clone());
        Ok(Some(path))
    };

    if cat.asset_kind() == AssetKind::HairParts {
        let requested = std::mem::take(&mut entry.hair_parts_model_unit);
        let slots: Vec<HairPartType> = if requested.is_empty() {
            HairPartType::KNOWN.to_vec()
        } else {
            requested.keys().copied().collect()
        };

        for slot in slots {
            let suffix = format!("_{}", slot.file_suffix());
            let unit = requested.get(&slot);
            let Some(model) = pick(unit, &|n| n.ends_with(&suffix))? else {
                anyhow::ensure!(unit.is_none(), "'{source}' has no model for slot {slot:?}");
                continue;
            };

            let phcl = cloth_path(unit.and_then(|u| u.phcl.as_deref()), &suffix)?;
            entry.hair_parts_model_unit.insert(
                slot,
                PartsModelUnitParam {
                    fmdb: Some(fmdb(&model)),
                    phcl,
                },
            );
        }

        anyhow::ensure!(
            !entry.hair_parts_model_unit.is_empty(),
            "'{source}' has no models named after a hair part slot (e.g. *_UpperLeft)"
        );

        entry.model_unit = PartsModelUnitParam::default();
    } else {
        let is_hat = |n: &str| n.contains("Hat");
        let is_flip = |n: &str| n.ends_with("_Flip");

        let hat = pick(entry.model_unit_for_hat.as_ref(), &|n| {
            is_hat(n) && !is_flip(n)
        })?;
        let flip = pick(entry.flipped_model_unit.as_ref(), &|n| {
            is_flip(n) && !is_hat(n)
        })?;
        let flip_hat = pick(entry.flipped_model_unit_for_hat.as_ref(), &|n| {
            is_hat(n) && is_flip(n)
        })?;
        let others = [&hat, &flip, &flip_hat];
        let main = names
            .iter()
            .find(|n| !others.iter().any(|o| o.as_deref() == Some(n.as_str())))
            .context("no model left for the main model unit")?
            .clone();

        let given = [
            Some(entry.model_unit.clone()),
            entry.model_unit_for_hat.clone(),
            entry.flipped_model_unit.clone(),
            entry.flipped_model_unit_for_hat.clone(),
        ];

        let mut built = Vec::new();
        for ((model, given), suffix) in [Some(main), hat, flip, flip_hat]
            .into_iter()
            .zip(given)
            .zip(["", "_Hat", "_Flip", "_HatFlip"])
        {
            built.push(match model {
                Some(model) => Some(PartsModelUnitParam {
                    fmdb: Some(fmdb(&model)),
                    phcl: cloth_path(given.and_then(|u| u.phcl).as_deref(), suffix)?,
                }),
                None => None,
            });
        }

        let [main, hat, flip, flip_hat] = <[_; 4]>::try_from(built).unwrap();
        entry.model_unit = main.unwrap_or_default();
        entry.model_unit_for_hat = hat;
        entry.flipped_model_unit = flip;
        entry.flipped_model_unit_for_hat = flip_hat;
    }

    bfres.name = token.to_string();
    files.push((
        format!("Model/{token}.bfres"),
        bfres.write().context("BFRES serialization failed")?,
    ));

    Ok(files)
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

    let mut orders = BTreeMap::new();
    let mut ids = BTreeMap::new();
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

    for cat in assets
        .iter()
        .filter_map(|spec| registry.get(&spec.category))
    {
        for order in cat.parts_orders() {
            if let std::collections::btree_map::Entry::Vacant(slot) = orders.entry(order.path) {
                let doc = byml_parse(&read_and_decompress(base, slot.key())?)?;
                slot.insert(doc);
            }
        }
    }

    let mut queue: Vec<&PartsEntry> = assets.iter().collect();
    queue.sort_by_key(|spec| {
        registry
            .get(&spec.category)
            .is_some_and(|cat| cat.asset_kind() != AssetKind::HairParts)
    });

    for (i, spec) in queue.into_iter().enumerate() {
        let cat = registry.get(&spec.category).expect("pre-verified");
        println!(
            "[{}/{}] Processing '{}' asset",
            i + 1,
            assets.len(),
            spec.category
        );

        let mut state = MutableState {
            rstbl_doc: &mut rstbl_doc,
            pack_entries: &mut pack_entries,
            rsizetable: &mut rsizetable,
            orders: &mut orders,
            base_bntx: base_bntx.as_mut(),
            editor_bntx: &mut editor_bntx,
            linked: &mut linked,
            ids: &mut ids,
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

    for (order_path, doc) in orders {
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

fn attach_hair_parts(entry: &mut PartsEntry, state: &mut MutableState) -> Result<Vec<(u32, u32)>> {
    let mut attached = Vec::new();

    if entry.hair_parts_attach_info.is_empty() {
        return Ok(attached);
    }

    let mut infos = std::mem::take(&mut entry.hair_parts_attach_info);
    for info in &mut infos {
        let name = match info.hair_parts_name.strip_prefix('@') {
            Some(id) => state.ids.get(id).cloned().with_context(|| {
                format!("'{}' attaches unknown asset id '{id}'", entry.file_name)
            })?,
            None => info.hair_parts_name.clone(),
        };

        let row = part_row(state.rstbl_doc, &name)?
            .with_context(|| format!("'{}' attaches unknown part '{name}'", entry.file_name))?;

        anyhow::ensure!(
            str_field(&row, "Category") == "HairParts",
            "'{name}' is not a HairParts part"
        );

        let index = i32_field(&row, "PartsIndex").context("HairParts without PartsIndex")? as u32;
        let fallback_icon = str_field(&row, "EditorIconName");
        let slots = [
            (
                info.is_attachable_upper,
                &mut info.editor_upper_icon_name,
                'U',
            ),
            (
                info.is_attachable_middle,
                &mut info.editor_middle_icon_name,
                'C',
            ),
            (
                info.is_attachable_lower,
                &mut info.editor_lower_icon_name,
                'D',
            ),
        ];

        for (slot, (attachable, icon, letter)) in slots.into_iter().enumerate() {
            if !attachable {
                icon.clear();
                continue;
            }

            attached.push((index, slot as u32));

            if icon.is_empty() {
                *icon = fallback_icon.clone();
            } else if !icon.starts_with("MiiEditor_") {
                let target = attach_icon_name(&entry.file_name, &name, letter);
                println!("Injecting hair parts icon: '{target}'");
                inject_texture(state.editor_bntx, icon, &target, ATTACH_ICON_TEMPLATE)?;

                *icon = target;
            }
        }

        entry.is_attachable_hair_parts_upper |= info.is_attachable_upper;
        entry.is_attachable_hair_parts_middle |= info.is_attachable_middle;
        entry.is_attachable_hair_parts_lower |= info.is_attachable_lower;

        info.hair_parts_name = name;
    }

    entry.is_use_hair_parts_model = true;
    entry.hair_parts_attach_info = infos;

    Ok(attached)
}

const ATTACH_ICON_TEMPLATE: &str = "MiiEditor_Face_HairBack000Parts000_C_Uit";

fn part_row(doc: &Byml, file_name: &str) -> Result<Option<BTreeMap<String, Value>>> {
    let row_id = format!("Work/Mii/Parts/{file_name}.mii__Parts.gyml");

    Ok(byml_root_array(doc)?.iter().find_map(|row| match row {
        Value::Dict(map) if row_id_of(row) == row_id => Some(map.clone()),
        _ => None,
    }))
}

fn template_row(
    doc: &Byml,
    cat: &dyn CategoryDef,
    template: &str,
) -> Result<BTreeMap<String, Value>> {
    let row = part_row(doc, template)?
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
