use crate::category::CategoryDef;
use crate::rstbl::alloc_size;
use crate::util::*;
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tomolib::formats::bntx::image::{self, encode_mips_swizzled};
use tomolib::formats::bntx::{Bntx, Texture};
use tomolib::formats::byml::{Byml, Value};
use tomolib::formats::rstbl::{PathEntry, Rstbl};

pub fn run(
    base: PathBuf,
    cat: Box<dyn CategoryDef>,
    asset_source: String,
    icon: Option<PathBuf>,
    mut out: PathBuf,
    order_index: Option<usize>,
) -> Result<()> {
    out.push("romfs/");

    let pack_bytes = read_and_decompress(&base, PATH_MII_PARTS_PACK)?;
    let pack_sarc = sarc_parse(pack_bytes)?;
    let mut pack_entries = sarc_entries_owned(&pack_sarc);

    let rstbl_bytes = read_and_decompress(&base, PATH_RSTBL_BYML)?;
    let mut rstbl_doc = byml_parse(&rstbl_bytes)?;

    let order_bytes = read_and_decompress(&base, cat.path_parts_order())?;
    let mut order_doc = byml_parse(&order_bytes)?;

    let rsizetable_bytes = read_and_decompress(&base, PATH_RSIZETABLE)?;
    let mut rsizetable = Rstbl::parse(&rsizetable_bytes).context("rsizetable parse")?;

    let existing_indices = collect_category_indices(&rstbl_doc, cat.parts_type_hash())?;
    let next_index = existing_indices.iter().copied().max().unwrap_or(0);
    let next_index = std::cmp::max(next_index, cat.vanilla_max_parts_index() as u32) + 1;

    let part_filename = cat.part_name(next_index);
    println!(
        "Adding new custom {} asset '{}' → {}",
        cat.category_name(),
        part_filename,
        out.display()
    );

    let target_internal_token = cat.internal_model_name(next_index);

    let editor_icon_name = match &icon {
        Some(_) => cat.editor_icon_name(next_index),
        None => cat.vanilla_icon_fallback().to_string(),
    };

    let row_id = cat.row_id(next_index);
    let mut initial_model_fmdb = String::new();
    if asset_source.ends_with(".bfres") || asset_source.ends_with(".bfres.zs") {
        let model_bytes = read_and_decompress(Path::new(""), &asset_source)?;
        let model_bfres = bfres_parse(&model_bytes)?;
        let model_internal_mesh_name = &model_bfres.models.names[0];

        let folder_name = cat.internal_model_name(next_index);

        let path_prefix: String = folder_name
            .chars()
            .take_while(|c| !c.is_ascii_digit())
            .collect();

        initial_model_fmdb = format!(
            "Work/Model/Mii/{path_prefix}/{folder_name}/output/{model_internal_mesh_name}.fmdb",
        );
    }

    let template = find_template_entry(&rstbl_doc, cat.category_name())?;
    let mut entry = cat.new_entry(
        next_index,
        initial_model_fmdb,
        Some(editor_icon_name.clone()),
        &template,
    );

    let mut written_sizes: BTreeMap<String, usize> = BTreeMap::new();

    if asset_source.ends_with(".bfres") || asset_source.ends_with(".bfres.zs") {
        let model_bytes = read_and_decompress(Path::new(""), &asset_source)?;
        let mut model_bfres = bfres_parse(&model_bytes)?;

        println!(
            "Renaming internal BFRES tracking property '{}' → '{target_internal_token}'",
            model_bfres.name
        );
        model_bfres.name = target_internal_token.clone();

        let model_internal_mesh_name = &model_bfres.models.names[0];
        let mesh_path = format!(
            "Work/Model/Mii/MiiHead/{target_internal_token}/output/{model_internal_mesh_name}.fmdb"
        );
        entry.model_paths.push(mesh_path);

        let target_bfres_filename = format!("{target_internal_token}.bfres");
        let out_model_relative_path = format!("Model/{target_bfres_filename}.zs");
        let out_model_lookup_key = format!("Model/{target_bfres_filename}");

        let serialized_bfres_payload = model_bfres.write().context("BFRES serialisation failed")?;
        let model_size =
            compress_and_write(&out, &out_model_relative_path, &serialized_bfres_payload)?;

        let model_path_entry = PathEntry {
            size: alloc_size(&out_model_lookup_key, model_size)?,
            name: out_model_lookup_key.clone(),
        };

        let mut path_entries = rsizetable.path_entries().to_vec();
        path_entries.push(model_path_entry);
        path_entries.sort_by_key(|f| f.name.clone());
        rsizetable.set_path_entries(path_entries);
    } else {
        let parts_bntx_rel = Path::new("romfs").join("Tex/Pack/MiiParts.bntx.zs");
        let raw_bntx_bytes = read_and_decompress(&base, parts_bntx_rel.to_str().unwrap())?;
        let mut base_bntx =
            Bntx::parse(&raw_bntx_bytes).context("Failed to parse base MiiParts.bntx")?;

        let png_bytes = std::fs::read(&asset_source)
            .with_context(|| format!("Failed to read asset source texture: {}", asset_source))?;
        let img = image::png_to_rgba(&png_bytes)?;

        let template_tex = base_bntx
            .textures
            .iter()
            .find(|tex| cat.matches_texture_name(&tex.name))
            .context("MiiParts.bntx has no matching category textures to clone layout rules from")?
            .clone();

        let swizzled_data = encode_mips_swizzled(&img, &template_tex)?;
        let new_tex = Texture {
            name: target_internal_token.clone(),
            info: template_tex.info,
            mip_offsets: template_tex.mip_offsets,
            user_data: vec![],
            image_data: swizzled_data,
        };

        base_bntx.textures.push(new_tex);
        let merged_bntx_bytes = base_bntx
            .write()
            .context("BNTX execution compression failed")?;
        let size = compress_and_write(&out, parts_bntx_rel.to_str().unwrap(), &merged_bntx_bytes)?;
        written_sizes.insert(clean_name("romfs/Tex/Pack/MiiParts.bntx").to_string(), size);
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
        row_id.clone(),
        &old_token,
        &target_internal_token,
        cat.as_ref(),
    );

    let root_array = byml_root_array_mut(&mut rstbl_doc)?;
    root_array.push(entry.build_rstbl_value());

    root_array.sort_by(|a, b| {
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

    let inner_bytes = byml_to_bytes(&Byml {
        version: 7,
        endian: tomolib::formats::byml::Endian::Little,
        root: entry.build_pack_value(),
    })?;
    pack_entries.insert(cat.pack_path(&part_filename), inner_bytes);

    {
        let root = byml_root_dict_mut(&mut order_doc)?;
        let order_arr = match root.get_mut("Order") {
            Some(Value::Array(a)) => a,
            _ => bail!("PartsOrder missing 'Order' Array"),
        };
        let insert_pos = order_index.unwrap_or(order_arr.len());
        if insert_pos > order_arr.len() {
            bail!(
                "--order-index {insert_pos} is out of range (max {})",
                order_arr.len()
            );
        }
        order_arr.insert(insert_pos, Value::U32(next_index));
    }

    let new_pack_bytes = sarc_pack(&pack_entries, &pack_sarc)?;
    let new_rstbl_bytes = byml_to_bytes(&rstbl_doc)?;
    let new_order_bytes = byml_to_bytes(&order_doc)?;

    let mii_pack_size = compress_and_write(&out, PATH_MII_PARTS_PACK, &new_pack_bytes)?;
    let rstbl_size = compress_and_write(&out, PATH_RSTBL_BYML, &new_rstbl_bytes)?;
    let parts_order_size = compress_and_write(&out, cat.path_parts_order(), &new_order_bytes)?;

    patch_rstbl(&mut rsizetable, PATH_RSTBL_BYML, rstbl_size)?;
    patch_rstbl(&mut rsizetable, cat.path_parts_order(), parts_order_size)?;
    patch_rstbl(&mut rsizetable, PATH_MII_PARTS_PACK, mii_pack_size)?;

    for (clean_path, decompressed_size) in &written_sizes {
        patch_rstbl(&mut rsizetable, clean_path, *decompressed_size)?;
    }

    if let Some(icon_path) = icon {
        let bntx_bytes = inject_bntx_icon(&base, &icon_path, &editor_icon_name, cat.as_ref())?;
        let bntx_size = compress_and_write(&out, PATH_MII_EDITOR_ICON, &bntx_bytes)?;
        patch_rstbl(&mut rsizetable, PATH_MII_EDITOR_ICON, bntx_size)?;
    }

    let mut rsizetable_buf = Vec::new();
    rsizetable
        .write(&mut rsizetable_buf)
        .context("rsizetable write")?;
    compress_and_write(&out, PATH_RSIZETABLE, &rsizetable_buf)?;

    println!("Custom mod successfully written out to {}", out.display());
    Ok(())
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
            "rstbl.byml has no existing {} entries to template from",
            target_category
        ))
}

fn inject_bntx_icon(
    base: &Path,
    icon_png: &Path,
    texture_name: &str,
    cat: &dyn CategoryDef,
) -> Result<Vec<u8>> {
    println!("Injecting custom editor icon: '{texture_name}'");
    let bntx_bytes = read_and_decompress(base, PATH_MII_EDITOR_ICON)?;
    let png_bytes = std::fs::read(icon_png)
        .with_context(|| format!("failed to read icon preview: {}", icon_png.display()))?;
    let img = image::png_to_rgba(&png_bytes)?;

    let mut bntx = Bntx::parse(&bntx_bytes).context("BNTX parse failed")?;
    bntx.textures.retain(|t| t.name != texture_name);

    let template = bntx.textures.iter()
        .find(|tex| cat.matches_icon_name(&tex.name))
        .with_context(|| format!(
            "MiiEditorIcon.bntx has no baseline vanilla textures matching the filter rules for category '{}'",
            cat.category_name()
        ))?.clone();

    println!(
        "Targeted layout template configuration: '{}'",
        template.name
    );

    let swizzled_data = encode_mips_swizzled(&img, &template)?;
    bntx.textures.push(Texture {
        name: texture_name.to_string(),
        info: template.info,
        mip_offsets: template.mip_offsets,
        user_data: vec![],
        image_data: swizzled_data,
    });

    bntx.write().context("BNTX serialization failed")
}
