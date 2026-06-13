use anyhow::{Context, Result};
use std::io::Cursor;
use tomolib::formats::rstbl::Rstbl;
use tomolib::formats::{bfres, sarc, zs};

pub const PATH_MII_PARTS_PACK: &str = "Pack/MiiParts.pack.zs";
pub const PATH_RSTBL_BYML: &str = "RSDB/MiiParts.Product.100.rstbl.byml.zs";
pub const PATH_RSIZETABLE: &str =
    "System/Resource/ResourceSizeTable.Product.100.Nin_NX_NVN.rsizetable.zs";
pub const PATH_MII_EDITOR_ICON: &str = "Tex/Pack/MiiEditorIcon.bntx.zs";

/// Returns true if a romfs path is zstd-compressed (i.e. ends with .zs).
pub fn is_zs(path: &str) -> bool {
    path.ends_with(".zs")
}

/// Read a file from disk, decompressing it if its path ends with .zs.
pub fn read_and_decompress(base: &std::path::Path, rel: &str) -> Result<Vec<u8>> {
    let raw = std::fs::read(base.join(rel)).with_context(|| format!("reading {rel}"))?;
    if is_zs(rel) {
        zs_decompress(&raw)
    } else {
        Ok(raw)
    }
}

/// Compress bytes if the destination path ends with .zs, then write to disk.
pub fn compress_and_write(out: &std::path::Path, rel: &str, bytes: &[u8]) -> Result<usize> {
    let data = if is_zs(rel) {
        zs_compress(bytes)?
    } else {
        bytes.to_vec()
    };

    let mods = std::path::Path::new("mods");
    let dest = mods.join(out).join(rel);

    if let Some(p) = dest.parent() {
        std::fs::create_dir_all(p)?;
    }

    std::fs::write(&dest, &data).with_context(|| format!("writing {}", dest.display()))?;
    println!("wrote {rel}");
    Ok(bytes.len())
}

pub fn zs_decompress(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    zs::decompress(&mut Cursor::new(bytes), &mut out).context("zstd decompress failed")?;
    Ok(out)
}

pub fn zs_compress(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();

    zs::compress(
        &mut Cursor::new(bytes),
        &mut out,
        zs::DEFAULT_LEVEL,
        Some(bytes.len() as u64),
    )
    .context("zstd compress failed")?;
    Ok(out)
}

pub fn patch_rstbl(rsizetable: &mut Rstbl, name: &str, size: usize) -> Result<()> {
    let clean_name = clean_name(name);

    let calculated_size = alloc_size(clean_name, size)?;

    rsizetable.set(clean_name, calculated_size);

    Ok(())
}

pub fn clean_name(name: &str) -> &str {
    name.strip_suffix(".zs").unwrap_or(name)
}

/// Parse a bfres from raw (already-decompressed) bytes.
pub fn bfres_parse(bytes: &[u8]) -> Result<bfres::Bfres> {
    bfres::Bfres::parse(bytes).context("BFRES parse failed")
}

/// Parse a SARC from raw (already-decompressed) bytes.
pub fn sarc_parse(bytes: Vec<u8>) -> Result<sarc::Sarc> {
    sarc::Sarc::parse(bytes).context("SARC parse failed")
}

/// Collect all entries from a SARC into an owned map: name → data.
/// Entries without names are skipped with a warning.
pub fn sarc_entries_owned(arc: &sarc::Sarc) -> indexmap::IndexMap<String, Vec<u8>> {
    let mut map = indexmap::IndexMap::new();
    for entry in arc.entries() {
        match &entry.name {
            Some(n) => {
                map.insert(n.clone(), arc.data(entry).to_vec());
            }
            None => eprintln!("warning: SARC entry has no name, skipping"),
        }
    }
    map
}

/// Repack an entry map into a SARC byte buffer.
/// `sarc::write` signature: write(&mut W, &[PackEntry<'_>], ByteOrder, u32) -> Result<u64>
pub fn sarc_pack(
    entries: &indexmap::IndexMap<String, Vec<u8>>,
    reference: &sarc::Sarc,
) -> Result<Vec<u8>> {
    let pack_entries: Vec<sarc::PackEntry<'_>> = entries
        .iter()
        .map(|(name, data)| sarc::PackEntry {
            name: name.as_str(),
            data: data.as_slice(),
        })
        .collect();
    let mut buf = Vec::new();
    sarc::write(&mut buf, &pack_entries, reference.byte_order(), 4).context("SARC pack failed")?;
    Ok(buf)
}

use tomolib::formats::byml::{Byml, Value};

use crate::rstbl::alloc_size;

pub fn byml_parse(bytes: &[u8]) -> Result<Byml> {
    Byml::parse(bytes).context("BYML parse failed")
}

pub fn byml_to_bytes(doc: &Byml) -> Result<Vec<u8>> {
    doc.to_bytes().context("BYML serialise failed")
}

pub fn byml_root_array(doc: &Byml) -> Result<&Vec<Value>> {
    match &doc.root {
        Value::Array(v) => Ok(v),
        _ => anyhow::bail!("expected BYML Array root"),
    }
}

pub fn byml_root_array_mut(doc: &mut Byml) -> Result<&mut Vec<Value>> {
    match &mut doc.root {
        Value::Array(v) => Ok(v),
        _ => anyhow::bail!("expected BYML Array root"),
    }
}

pub fn byml_root_dict(doc: &Byml) -> Result<&std::collections::BTreeMap<String, Value>> {
    match &doc.root {
        Value::Dict(m) => Ok(m),
        _ => anyhow::bail!("expected BYML Dict root"),
    }
}

pub fn byml_root_dict_mut(
    doc: &mut Byml,
) -> Result<&mut std::collections::BTreeMap<String, Value>> {
    match &mut doc.root {
        Value::Dict(m) => Ok(m),
        _ => anyhow::bail!("expected BYML Dict root"),
    }
}
