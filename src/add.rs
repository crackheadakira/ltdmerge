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
    model: String,
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

    let order_bytes = read_and_decompress(&base, PATH_PARTS_ORDER)?;
    let mut order_doc = byml_parse(&order_bytes)?;

    let model_bytes = read_and_decompress(Path::new(""), &model)?;
    let mut model_bfres = bfres_parse(&model_bytes)?;

    let rsizetable_bytes = read_and_decompress(&base, PATH_RSIZETABLE)?;
    let mut rsizetable = Rstbl::parse(&rsizetable_bytes).context("rsizetable parse")?;

    let existing_indices = collect_parts_indices(&rstbl_doc)?;
    let next_index = existing_indices.iter().copied().max().unwrap_or(0) + 1;

    let name = format!("Faceline{next_index}");
    println!("Adding head '{name}' → {}", out.display());

    let target_head_name = format!("MiiHead{next_index}");
    let target_bfres_filename = format!("{target_head_name}.bfres");

    println!(
        "Renaming internal BFRES property '{}' → '{target_head_name}'",
        model_bfres.name
    );
    model_bfres.name = target_head_name.clone();

    let model_internal_mesh_name = &model_bfres.models.names[0];
    let model_entry =
        format!("Work/Model/Mii/MiiHead/{target_head_name}/output/{model_internal_mesh_name}.fmdb");

    println!("Inserting metadata tracking key: {model_entry}");

    let template = find_template_entry(&rstbl_doc)?;

    let editor_icon_name = match &icon {
        Some(_) => {
            format!("MiiEditor_Face_Faceline{next_index}_Uit")
        }
        None => "MiiEditor_Face_Faceline15_Uit".to_string(),
    };

    let row_id = format!("Work/Mii/Parts/{name}.mii__Parts.gyml");

    let new_rstbl_entry = build_rstbl_entry(
        &template,
        &name,
        &model_entry,
        &editor_icon_name,
        &row_id,
        next_index,
    )?;

    let root_array = byml_root_array_mut(&mut rstbl_doc)?;
    root_array.push(new_rstbl_entry);

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

    let inner_bgyml = build_inner_parts_bgyml(&name, &model_entry, &editor_icon_name, next_index)?;
    let inner_bytes = byml_to_bytes(&inner_bgyml)?;
    let inner_path = pack_parts_path(&name);

    pack_entries.insert(inner_path, inner_bytes);

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

    let serialized_bfres_payload = model_bfres.write().context("BFRES serialisation failed")?;

    let bntx_result = if let Some(icon_path) = icon {
        Some(inject_bntx_icon(&base, &icon_path, &editor_icon_name)?)
    } else {
        None
    };

    let out_model_relative_path = format!("Model/{target_bfres_filename}.zs");
    let out_model_lookup_key = format!("Model/{target_bfres_filename}");

    let mii_pack_size = compress_and_write(&out, PATH_MII_PARTS_PACK, &new_pack_bytes)?;
    let rstbl_size = compress_and_write(&out, PATH_RSTBL_BYML, &new_rstbl_bytes)?;
    let parts_order_size = compress_and_write(&out, PATH_PARTS_ORDER, &new_order_bytes)?;

    let model_size = compress_and_write(&out, &out_model_relative_path, &serialized_bfres_payload)?;

    let model_path_entry = PathEntry {
        size: alloc_size(&out_model_lookup_key, model_size)?,
        name: out_model_lookup_key,
    };

    patch_rstbl(&mut rsizetable, PATH_RSTBL_BYML, rstbl_size)?;
    patch_rstbl(&mut rsizetable, PATH_PARTS_ORDER, parts_order_size)?;
    patch_rstbl(&mut rsizetable, PATH_MII_PARTS_PACK, mii_pack_size)?;

    let mut path_entries = rsizetable.path_entries().to_vec();
    path_entries.push(model_path_entry);
    path_entries.sort_by_key(|f| f.name.clone());
    rsizetable.set_path_entries(path_entries);

    if let Some(bntx_bytes) = bntx_result {
        let bntx_size = compress_and_write(&out, PATH_MII_EDITOR_ICON, &bntx_bytes)?;
        patch_rstbl(&mut rsizetable, PATH_MII_EDITOR_ICON, bntx_size)?;
    }

    let mut rsizetable_buf = Vec::new();
    rsizetable
        .write(&mut rsizetable_buf)
        .context("rsizetable write")?;
    compress_and_write(&out, PATH_RSIZETABLE, &rsizetable_buf)?;

    println!("Mod written out to {}", out.display());
    Ok(())
}

fn collect_parts_indices(doc: &Byml) -> Result<Vec<u32>> {
    byml_root_array(doc)?
        .iter()
        .filter_map(|entry| {
            if let Value::Dict(m) = entry {
                if let Some(Value::U32(parts_type)) = m.get("PartsType") {
                    if *parts_type == 780281636 {
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

pub fn find_template_entry(doc: &Byml) -> Result<BTreeMap<String, Value>> {
    byml_root_array(doc)?
        .iter()
        .filter_map(|entry| {
            if let Value::Dict(m) = entry {
                if let Some(Value::String(cat)) = m.get("Category") {
                    if cat == "Faceline" {
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
        .context("rstbl.byml has no Faceline entries to template from")
}

fn build_rstbl_entry(
    template: &BTreeMap<String, Value>,
    name: &str,
    model: &str,
    editor_icon_name: &str,
    row_id: &str,
    parts_index: u32,
) -> Result<Value> {
    let mut m = template.clone();

    set_str(&mut m, "FileName", name);
    set_str(&mut m, "EditorIconName", editor_icon_name);
    set_str(&mut m, "__RowId", row_id);
    set_i32(&mut m, "PartsIndex", parts_index as i32);

    if let Some(Value::Dict(mu)) = m.get_mut("ModelUnit") {
        mu.insert("Fmdb".into(), Value::String(model.into()));
    }

    Ok(Value::Dict(m))
}

fn build_inner_parts_bgyml(
    name: &str,
    model: &str,
    editor_icon_name: &str,
    parts_index: u32,
) -> Result<Byml> {
    let mut mu = BTreeMap::new();
    mu.insert("Fmdb".into(), Value::String(model.into()));

    let mut root = BTreeMap::new();
    root.insert("Category".into(), Value::String("Faceline".into()));
    root.insert(
        "EditorIconName".into(),
        Value::String(editor_icon_name.into()),
    );
    root.insert("FileName".into(), Value::String(name.into()));
    root.insert("IsVisibleInEditor".into(), Value::Bool(true));
    root.insert("ModelUnit".into(), Value::Dict(mu));
    root.insert("PartsIndex".into(), Value::I32(parts_index as i32));

    Ok(Byml {
        version: 7,
        endian: tomolib::formats::byml::Endian::Little,
        root: Value::Dict(root),
    })
}

fn inject_bntx_icon(base: &Path, icon_png: &Path, texture_name: &str) -> Result<Vec<u8>> {
    println!(
        "injecting icon '{texture_name}' from {}",
        icon_png.display()
    );

    let bntx_bytes = read_and_decompress(base, PATH_MII_EDITOR_ICON)?;
    let png_bytes =
        std::fs::read(icon_png).with_context(|| format!("reading icon: {}", icon_png.display()))?;

    let img = image::png_to_rgba(&png_bytes)?;

    let mut bntx = Bntx::parse(&bntx_bytes).context("BNTX parse failed")?;

    if bntx.textures.iter().any(|t| t.name == texture_name) {
        eprintln!("  warning: BNTX already contains '{texture_name}', replacing");
        bntx.textures.retain(|t| t.name != texture_name);
    }

    let template = bntx
        .textures
        .iter()
        .find(|tex| tex.name.contains("Faceline"))
        .context("BNTX has no existing textures containing 'Faceline' to clone metadata from")?
        .clone();

    let swizzled_data = encode_mips_swizzled(&img, &template)?;

    let new_tex = Texture {
        name: texture_name.to_string(),
        info: template.info,
        mip_offsets: template.mip_offsets,
        user_data: vec![],
        image_data: swizzled_data,
    };

    bntx.textures.push(new_tex);
    bntx.write().context("BNTX serialise failed")
}

fn set_str(m: &mut BTreeMap<String, Value>, key: &str, val: &str) {
    m.insert(key.to_string(), Value::String(val.to_string()));
}

fn set_i32(m: &mut BTreeMap<String, Value>, key: &str, val: i32) {
    m.insert(key.to_string(), Value::I32(val));
}
