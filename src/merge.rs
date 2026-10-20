use crate::category::{CategoryDef, i32_field};
use crate::engine::{MiiMergeEngine, rename_resource};
use crate::types::CustomModNamespace;
use crate::util::*;
use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tomolib::formats::bntx::{Bntx, Texture};
use tomolib::formats::byml::{Byml, Value};
use tomolib::formats::rstbl::Rstbl;

pub fn run(
    mod_dirs: Vec<PathBuf>,
    out_dir: PathBuf,
    categories: Vec<Box<dyn CategoryDef>>,
) -> Result<()> {
    run_with_base(mod_dirs, out_dir, categories, None)
}

pub fn run_with_base(
    mod_dirs: Vec<PathBuf>,
    out_dir: PathBuf,
    mut categories: Vec<Box<dyn CategoryDef>>,
    base: Option<&Path>,
) -> Result<()> {
    anyhow::ensure!(
        mod_dirs.len() >= 2,
        "merge requires at least two input mod directories"
    );

    let destination = Path::new("mods").join(&out_dir);
    let destination = std::path::absolute(&destination)?;
    for input in &mod_dirs {
        let input = input.canonicalize()?;

        anyhow::ensure!(
            !destination.starts_with(&input),
            "output must not be inside an input mod"
        );
    }

    categories.sort_by(|a, b| a.category_name().cmp(b.category_name()));
    let mut mods = Vec::new();

    for (id, dir) in mod_dirs.iter().enumerate() {
        if let Some(m) = CustomModNamespace::load(id, dir, &categories)? {
            mods.push(m);
        }
    }
    let mut engine = MiiMergeEngine::new(mods, categories);
    let mut reserved: BTreeMap<String, BTreeSet<i32>> = BTreeMap::new();
    let mut row_template = None;
    let mut rows = BTreeMap::new();
    let mut baseline_rows = BTreeMap::new();

    if let Some(base) = base {
        anyhow::ensure!(base.is_dir(), "--base is not a directory");

        if asset_exists(base, PATH_RSTBL_BYML) {
            let doc = byml_parse(&read_and_decompress(base, PATH_RSTBL_BYML)?)?;
            baseline_rows = rows_by_name(&doc)?;
            for row in baseline_rows.values() {
                reserve_row(row, &engine.categories, &mut reserved);
            }
            rows = baseline_rows.clone();
            row_template = Some(doc);
        }
    }

    for dir in &mod_dirs {
        let rel = format!("romfs/{PATH_RSTBL_BYML}");
        if !asset_exists(dir, &rel) {
            continue;
        }

        let doc = byml_parse(&read_and_decompress(dir, &rel)?)?;
        for (name, row) in rows_by_name(&doc)? {
            if is_marked(&row) {
                continue;
            }
            reserve_row(&row, &engine.categories, &mut reserved);
            merge_named(&mut rows, name, row, &baseline_rows, "RSDB row");
        }

        row_template = Some(doc);
    }

    engine.reserve_indices(reserved);
    engine.compute_allocations()?;
    engine.execute_remap()?;

    let mut outputs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    if let Some(mut doc) = row_template {
        for m in &engine.mods {
            for cat in &engine.categories {
                if let Some(data) = m.categories.get(cat.category_name()) {
                    for row in &data.rstbl_entries {
                        row.validate()?;

                        let key = if row.row_id.is_empty() {
                            cat.row_id(row.parts_index as u32)
                        } else {
                            row.row_id.clone()
                        };

                        anyhow::ensure!(
                            !rows.contains_key(&key),
                            "remapped row collides with existing resource: {key}"
                        );

                        rows.insert(key, cat.build_rstbl_value(row));
                    }
                }
            }
        }

        doc.root = Value::Array(rows.into_values().collect());
        outputs.insert(PATH_RSTBL_BYML.into(), byml_to_bytes(&doc)?);
    }

    let mut pack_template = None;
    let mut members = BTreeMap::new();
    let mut baseline_members = BTreeMap::new();
    if let Some(base) = base.filter(|b| asset_exists(b, PATH_MII_PARTS_PACK)) {
        let pack = sarc_parse(read_and_decompress(base, PATH_MII_PARTS_PACK)?)?;
        baseline_members = sarc_entries_owned(&pack).into_iter().collect();
        members = baseline_members.clone();
        pack_template = Some(pack);
    }

    for (id, dir) in mod_dirs.iter().enumerate() {
        let rel = format!("romfs/{PATH_MII_PARTS_PACK}");
        if !asset_exists(dir, &rel) {
            continue;
        }

        let pack = sarc_parse(read_and_decompress(dir, &rel)?)?;

        for (name, bytes) in sarc_entries_owned(&pack) {
            let owned = owned_pack_remap(&engine, id, &name);
            let (name, bytes) = if let Some((cat, old, new)) = owned {
                if old == new {
                    (name, bytes)
                } else {
                    let entry = &engine
                        .mods
                        .iter()
                        .find(|m| m.mod_index == id)
                        .unwrap()
                        .categories[cat.category_name()]
                    .pack_parts[&new];

                    let source = byml_parse(&bytes)?;
                    let doc = Byml {
                        root: cat.build_pack_value(entry),
                        endian: source.endian,
                        version: source.version,
                    };

                    (
                        cat.pack_path(&cat.part_name(new as u32)),
                        byml_to_bytes(&doc)?,
                    )
                }
            } else {
                (name, bytes)
            };

            if owned.is_some() {
                anyhow::ensure!(
                    !members.contains_key(&name)
                        || baseline_members.get(&name) == members.get(&name),
                    "remapped packed part collides: {name}"
                );
                members.insert(name, bytes);
            } else {
                merge_named(&mut members, name, bytes, &baseline_members, "SARC member");
            }
        }

        pack_template = Some(pack);
    }

    if let Some(pack) = pack_template {
        let entries = members.into_iter().collect();
        outputs.insert(PATH_MII_PARTS_PACK.into(), sarc_pack(&entries, &pack)?);
    }

    for cat in &engine.categories {
        let rel = format!("romfs/{}", cat.path_parts_order());
        let mut template = None;
        let mut order = Vec::new();

        if let Some(base) = base.filter(|b| asset_exists(b, cat.path_parts_order())) {
            let bytes = read_and_decompress(base, cat.path_parts_order())?;
            order = parse_parts_order_array(&bytes)?;
            template = Some(byml_parse(&bytes)?);
        }

        for (id, dir) in mod_dirs.iter().enumerate() {
            if !asset_exists(dir, &rel) {
                continue;
            }

            let bytes = read_and_decompress(dir, &rel)?;
            let source = parse_parts_order_array(&bytes)?;
            let mapped: Vec<_> = source
                .into_iter()
                .map(|old| {
                    engine
                        .allocation_map
                        .get(&(cat.category_name().into(), id, old))
                        .copied()
                        .unwrap_or(old)
                })
                .collect();

            let previous_owned: Vec<_> = order
                .iter()
                .copied()
                .filter(|i| {
                    engine
                        .allocation_map
                        .iter()
                        .any(|((category, _, _), new)| category == cat.category_name() && new == i)
                })
                .collect();

            order = mapped;

            for index in previous_owned {
                if !order.contains(&index) {
                    order.push(index);
                }
            }

            template = Some(byml_parse(&bytes)?);
        }

        if let Some(mut doc) = template {
            let mut seen = BTreeSet::new();
            order.retain(|i| seen.insert(*i));
            byml_root_dict_mut(&mut doc)?.insert(
                "Order".into(),
                Value::Array(order.into_iter().map(|i| Value::U32(i as u32)).collect()),
            );
            outputs.insert(cat.path_parts_order().into(), byml_to_bytes(&doc)?);
        }
    }

    for (path, icons) in [
        (PATH_MII_EDITOR_ICON, true),
        ("Tex/Pack/MiiParts.bntx.zs", false),
    ] {
        if let Some(bytes) = merge_textures(path, icons, &mod_dirs, base, &engine)? {
            outputs.insert(path.into(), bytes);
        }
    }

    let managed: BTreeSet<_> = outputs
        .keys()
        .map(|s| clean_name(s).to_owned())
        .chain([clean_name(PATH_RSIZETABLE).to_owned()])
        .collect();
    let mut copied = BTreeMap::new();

    for (id, dir) in mod_dirs.iter().enumerate() {
        let model_names = engine.resource_renames(id, false);
        for entry in walkdir::WalkDir::new(dir.join("romfs")) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }

            let rel = entry
                .path()
                .strip_prefix(dir.join("romfs"))?
                .to_string_lossy()
                .replace('\\', "/");

            if managed.contains(clean_name(&rel)) {
                continue;
            }

            let mut key = rel.clone();
            if let Some(stem) = clean_name(&rel)
                .strip_prefix("Model/")
                .and_then(|s| s.strip_suffix(".bfres"))
            {
                let renamed = rename_resource(stem, &model_names);
                key = format!(
                    "Model/{renamed}.bfres{}",
                    if rel.ends_with(".zs") { ".zs" } else { "" }
                );
            }

            let bytes = read_and_decompress(&dir.join("romfs"), &rel)?;
            let baseline = if let Some(base) = base.filter(|b| asset_exists(b, &rel)) {
                Some(read_and_decompress(base, &rel)?)
            } else {
                None
            };

            if baseline.as_ref() == Some(&bytes) && copied.contains_key(&key) {
                continue;
            }

            if copied.get(&key).is_some_and(|old| old != &bytes) {
                eprintln!("warning: resource {key}: last input wins");
            }

            copied.insert(key, bytes);
        }
    }

    outputs.extend(copied);

    let mut table = None;
    let mut crc = BTreeMap::new();
    let mut paths = BTreeMap::new();
    let mut baseline_crc = BTreeMap::new();
    let mut baseline_paths = BTreeMap::new();

    if let Some(base) = base.filter(|b| asset_exists(b, PATH_RSIZETABLE)) {
        let t = Rstbl::parse(&read_and_decompress(base, PATH_RSIZETABLE)?)?;
        for e in t.crc_entries() {
            baseline_crc.insert(e.hash, e.size);
        }

        for e in t.path_entries() {
            baseline_paths.insert(e.name.clone(), e.size);
        }

        crc = baseline_crc.clone();
        paths = baseline_paths.clone();
        table = Some(t);
    }

    for dir in &mod_dirs {
        let rel = format!("romfs/{PATH_RSIZETABLE}");
        if !asset_exists(dir, &rel) {
            continue;
        }

        let t = Rstbl::parse(&read_and_decompress(dir, &rel)?)?;
        for e in t.crc_entries() {
            if baseline_crc.get(&e.hash) != Some(&e.size) || !crc.contains_key(&e.hash) {
                crc.insert(e.hash, e.size);
            }
        }

        for e in t.path_entries() {
            if baseline_paths.get(&e.name) != Some(&e.size) || !paths.contains_key(&e.name) {
                paths.insert(e.name.clone(), e.size);
            }
        }

        table = Some(t);
    }

    let mut table =
        table.context("merging requires a resource size table in an input mod or --base")?;
    table.set_crc_entries(
        crc.into_iter()
            .map(|(hash, size)| tomolib::formats::rstbl::CrcEntry { hash, size })
            .collect(),
    );

    table.set_path_entries(
        paths
            .into_iter()
            .map(|(name, size)| tomolib::formats::rstbl::PathEntry { name, size })
            .collect(),
    );

    for (path, bytes) in &outputs {
        patch_rstbl(&mut table, path, bytes.len())?;
    }

    let mut table_bytes = Vec::new();
    table.write(&mut table_bytes)?;
    outputs.insert(PATH_RSIZETABLE.into(), table_bytes);

    validate_output(&outputs, &engine, base)?;

    let encoded: Vec<_> = outputs
        .iter()
        .map(|(path, bytes)| {
            Ok((
                path,
                if path.ends_with(".zs") {
                    zs_compress(bytes)?
                } else {
                    bytes.clone()
                },
            ))
        })
        .collect::<Result<_>>()?;

    for (path, bytes) in encoded {
        let path = destination.join("romfs").join(path);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
    }

    println!(
        "Merged {} mods -> {}",
        mod_dirs.len(),
        destination.display()
    );

    Ok(())
}

fn is_marked(value: &Value) -> bool {
    matches!(value,Value::Dict(m) if matches!(m.get("IsLtdmergeGen"),Some(Value::Bool(true))))
}

fn rows_by_name(doc: &Byml) -> Result<BTreeMap<String, Value>> {
    let mut result = BTreeMap::new();
    for value in byml_root_array(doc)? {
        let Value::Dict(map) = value else {
            bail!("invalid MiiParts row")
        };

        let Some(Value::String(name)) = map.get("__RowId") else {
            bail!("MiiParts row missing __RowId")
        };

        anyhow::ensure!(
            result.insert(name.clone(), value.clone()).is_none(),
            "duplicate MiiParts row {name}"
        );
    }

    Ok(result)
}

fn reserve_row(
    value: &Value,
    categories: &[Box<dyn CategoryDef>],
    reserved: &mut BTreeMap<String, BTreeSet<i32>>,
) {
    let Value::Dict(map) = value else { return };
    let Some(index) = i32_field(map, "PartsIndex") else {
        return;
    };

    for cat in categories {
        if matches!(map.get("Category"),Some(Value::String(s)) if s==cat.internal_category_name()) {
            reserved
                .entry(cat.category_name().into())
                .or_default()
                .insert(index);
        }
    }
}

fn merge_named<T: PartialEq>(
    dest: &mut BTreeMap<String, T>,
    name: String,
    value: T,
    baseline: &BTreeMap<String, T>,
    kind: &str,
) {
    if baseline.get(&name) == Some(&value) && dest.contains_key(&name) {
        return;
    }

    if let Some(previous) = dest.get(&name) {
        if previous != &value && baseline.get(&name) != Some(previous) {
            eprintln!("warning: {kind} {name}: last input wins");
        }
    }

    dest.insert(name, value);
}

fn owned_pack_remap<'a>(
    engine: &'a MiiMergeEngine,
    id: usize,
    name: &str,
) -> Option<(&'a dyn CategoryDef, i32, i32)> {
    for cat in &engine.categories {
        for ((category, owner, old), new) in &engine.allocation_map {
            if category == cat.category_name()
                && *owner == id
                && name == cat.pack_path(&cat.part_name(*old as u32))
            {
                return Some((cat.as_ref(), *old, *new));
            }
        }
    }

    None
}

fn merge_textures(
    path: &str,
    icons: bool,
    dirs: &[PathBuf],
    base: Option<&Path>,
    engine: &MiiMergeEngine,
) -> Result<Option<Vec<u8>>> {
    let mut template = None;
    let mut textures: BTreeMap<String, Texture> = BTreeMap::new();
    let mut baseline = BTreeMap::new();

    if let Some(base) = base.filter(|b| asset_exists(b, path)) {
        let bntx = Bntx::parse(&read_and_decompress(base, path)?)?;
        for t in &bntx.textures {
            baseline.insert(t.name.clone(), t.clone());
        }

        textures = baseline.clone();
        template = Some(bntx);
    }

    for (id, dir) in dirs.iter().enumerate() {
        let rel = format!("romfs/{path}");
        if !asset_exists(dir, &rel) {
            continue;
        }

        let bntx = Bntx::parse(&read_and_decompress(dir, &rel)?)?;
        let names = engine.resource_renames(id, icons);
        for mut texture in bntx.textures.clone() {
            texture.name = rename_resource(&texture.name, &names);
            merge_named(
                &mut textures,
                texture.name.clone(),
                texture,
                &baseline,
                "texture",
            );
        }

        template = Some(bntx);
    }

    template
        .map(|mut bntx| {
            bntx.textures = textures.into_values().collect();
            Ok(bntx.write()?)
        })
        .transpose()
}

fn validate_output(
    outputs: &BTreeMap<String, Vec<u8>>,
    engine: &MiiMergeEngine,
    base: Option<&Path>,
) -> Result<()> {
    let icon_names = outputs
        .get(PATH_MII_EDITOR_ICON)
        .map(|b| {
            Bntx::parse(b).map(|b| {
                b.textures
                    .into_iter()
                    .map(|t| t.name)
                    .collect::<BTreeSet<_>>()
            })
        })
        .transpose()?;

    let texture_names = outputs
        .get("Tex/Pack/MiiParts.bntx.zs")
        .map(|b| {
            Bntx::parse(b).map(|b| {
                b.textures
                    .into_iter()
                    .map(|t| t.name)
                    .collect::<BTreeSet<_>>()
            })
        })
        .transpose()?;

    let members = outputs
        .get(PATH_MII_PARTS_PACK)
        .map(|b| sarc_parse(b.clone()).map(|p| sarc_entries_owned(&p)))
        .transpose()?;

    for m in &engine.mods {
        for cat in &engine.categories {
            if let Some(data) = m.categories.get(cat.category_name()) {
                for entry in &data.rstbl_entries {
                    if let Some(members) = &members {
                        anyhow::ensure!(
                            members.contains_key(&cat.pack_path(&entry.file_name)),
                            "missing packed part {}",
                            entry.file_name
                        );
                    }

                    for name in [&entry.editor_icon_name, &entry.editor_mask_icon_name]
                        .into_iter()
                        .flatten()
                        .filter(|s| !s.is_empty())
                    {
                        if let Some(names) = &icon_names {
                            anyhow::ensure!(names.contains(name), "missing editor icon {name}");
                        }
                    }

                    if let Some(name) = entry.texture_name.as_ref().filter(|s| !s.is_empty()) {
                        if let Some(names) = &texture_names {
                            anyhow::ensure!(names.contains(name), "missing parts texture {name}");
                        }
                    }

                    if base.is_some() && data.pack_parts.contains_key(&entry.parts_index) {
                        data.pack_parts[&entry.parts_index].validate()?;
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn parse_parts_order_array(bytes: &[u8]) -> Result<Vec<i32>> {
    let doc = byml_parse(bytes)?;
    let root = byml_root_dict(&doc)?;

    let Some(Value::Array(values)) = root.get("Order") else {
        bail!("PartsOrder missing Order array")
    };

    values
        .iter()
        .map(|v| match v {
            Value::I32(i) if *i >= 0 => Ok(*i),
            Value::U32(i) => Ok(i32::try_from(*i)?),
            _ => bail!("invalid PartsOrder index"),
        })
        .collect()
}
