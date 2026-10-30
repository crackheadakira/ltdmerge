use crate::category::{AssetKind, CategoryDef, HairPartType, PartsEntry, str_field};
use crate::registry::CategoryRegistry;
use crate::util::*;
use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use tomolib::formats::bntx::Bntx;
use tomolib::formats::bntx::image::{RgbaImage, can_encode, decode_texture_rgba};
use tomolib::formats::byml::{Byml, Value};

pub const ATTACH_ICON_TEMPLATE: &str = "MiiEditor_Face_HairBack000Parts000_C_Uit";

pub struct BaseAssets {
    pub parts: Byml,
    pub icons: Bntx,
    pub textures: Bntx,
}

pub struct TemplatePart {
    pub file_name: String,
    pub icon_name: String,
    pub row: BTreeMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct TextureSpec {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub encodable: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AssetRequirements {
    pub texture: Option<TextureSpec>,
    pub icon: Option<TextureSpec>,
    pub mask: Option<TextureSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitSlot {
    Main,
    Hat,
    Flip,
    FlipHat,
    HairPart(HairPartType),
}

#[derive(Debug, Clone)]
pub struct ResolvedUnit {
    pub slot: UnitSlot,
    pub model: String,
    pub phcl: Option<String>,
    pub cloth_suffix: String,
}

#[derive(Debug, Clone)]
pub struct Problem {
    pub asset: usize,
    pub field: &'static str,
    pub message: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "asset {} {}: {}",
            self.asset + 1,
            self.field,
            self.message
        )
    }
}

impl BaseAssets {
    pub fn load(base: &Path) -> Result<Self> {
        Ok(Self {
            parts: byml_parse(&read_and_decompress(base, PATH_RSTBL_BYML)?)?,
            icons: Bntx::parse(&read_and_decompress(base, PATH_MII_EDITOR_ICON)?)?,
            textures: Bntx::parse(&read_and_decompress(base, PATH_MII_PARTS_BNTX)?)?,
        })
    }

    pub fn templates(&self, cat: &dyn CategoryDef) -> Vec<TemplatePart> {
        let Ok(rows) = byml_root_array(&self.parts) else {
            return Vec::new();
        };

        let mut parts: Vec<TemplatePart> = rows
            .iter()
            .filter_map(|row| match row {
                Value::Dict(map)
                    if str_field(map, "Category") == cat.internal_category_name()
                        && cat.owns_file_name(&str_field(map, "FileName")) =>
                {
                    Some(TemplatePart {
                        file_name: str_field(map, "FileName"),
                        icon_name: str_field(map, "EditorIconName"),
                        row: map.clone(),
                    })
                }
                _ => None,
            })
            .collect();

        parts.sort_by(|a, b| a.file_name.cmp(&b.file_name));
        parts
    }

    pub fn editor_icon(&self, name: &str) -> Option<RgbaImage> {
        let texture = self.icons.textures.iter().find(|t| t.name == name)?;

        decode_texture_rgba(texture, 0).ok()
    }

    pub fn template(&self, cat: &dyn CategoryDef, name: &str) -> Result<BTreeMap<String, Value>> {
        template_row(&self.parts, cat, name)
    }

    pub fn requirements(&self, template: &BTreeMap<String, Value>) -> AssetRequirements {
        let icon = str_field(template, "EditorIconName");

        let mask = Some(str_field(template, "EditorMaskIconName"))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| icon.clone());

        AssetRequirements {
            texture: texture_spec(&self.textures, &str_field(template, "TextureName")),
            icon: texture_spec(&self.icons, &icon),
            mask: texture_spec(&self.icons, &mask),
        }
    }

    pub fn check(&self, assets: &[PartsEntry], registry: &CategoryRegistry) -> Vec<Problem> {
        let mut problems = Vec::new();
        let mut ids = BTreeSet::new();

        for (index, asset) in assets.iter().enumerate() {
            if let Some(id) = &asset.id
                && !ids.insert(id.clone())
            {
                problems.push(Problem {
                    asset: index,
                    field: "id",
                    message: format!("id '{id}' is used twice"),
                });
            }
        }

        let attach_icon = texture_spec(&self.icons, ATTACH_ICON_TEMPLATE);

        for (index, asset) in assets.iter().enumerate() {
            let mut report = |field: &'static str, message: String| {
                problems.push(Problem {
                    asset: index,
                    field,
                    message,
                })
            };

            let Some(cat) = registry.get(&asset.category) else {
                report("category", format!("unknown category '{}'", asset.category));
                continue;
            };

            let template_name = asset
                .template
                .clone()
                .unwrap_or_else(|| cat.template_part().to_string());
            let template = match self.template(cat, &template_name) {
                Ok(row) => row,
                Err(err) => {
                    report("template", err.to_string());
                    continue;
                }
            };

            let needs = self.requirements(&template);
            check_png(
                &mut report,
                "editor_icon_name",
                asset.editor_icon_name.as_deref(),
                needs.icon.as_ref(),
            );

            if cat.editor_mask_icon_name(0).is_some() {
                check_png(
                    &mut report,
                    "editor_mask_icon_name",
                    asset.editor_mask_icon_name.as_deref(),
                    needs.mask.as_ref(),
                );
            }

            match cat.asset_kind() {
                AssetKind::Texture => {
                    let Some(source) = asset.texture_name.as_deref() else {
                        report("texture_name", "a texture PNG is required".into());
                        continue;
                    };

                    check_png(
                        &mut report,
                        "texture_name",
                        Some(source),
                        needs.texture.as_ref(),
                    );

                    check_png(
                        &mut report,
                        "no_lip_texture_name",
                        asset.no_lip_texture_name.as_deref(),
                        needs.texture.as_ref(),
                    );

                    if let Some(spec) = &needs.texture
                        && !spec.encodable
                    {
                        report(
                            "texture_name",
                            format!("{} textures can't be encoded", spec.format),
                        );
                    }
                }
                AssetKind::Model | AssetKind::HairParts => {
                    let Some(source) = asset.model_unit.fmdb.as_deref() else {
                        report(
                            "model_unit",
                            "a .bfres is required in model_unit.fmdb".into(),
                        );

                        continue;
                    };

                    match bfres_models(Path::new(source)) {
                        Ok(names) => match resolve_models(asset, cat, &names) {
                            Ok(units) => {
                                for phcl in units.iter().filter_map(|u| u.phcl.as_deref()) {
                                    if !phcl.starts_with("Work/") && !Path::new(phcl).is_file() {
                                        report("phcl", format!("'{phcl}' does not exist"));
                                    }
                                }
                            }
                            Err(err) => report("model_unit", err.to_string()),
                        },
                        Err(err) => report("model_unit", format!("{err:#}")),
                    }
                }
            }

            for info in &asset.hair_parts_attach_info {
                match info.hair_parts_name.strip_prefix('@') {
                    Some(id) if !ids.contains(id) => {
                        report("hair_parts_attach_info", format!("no asset has id '{id}'"))
                    }
                    Some(_) => {}
                    None => {
                        if part_row(&self.parts, &info.hair_parts_name)
                            .ok()
                            .flatten()
                            .is_none_or(|row| str_field(&row, "Category") != "HairParts")
                        {
                            report(
                                "hair_parts_attach_info",
                                format!("'{}' is not a vanilla HairParts", info.hair_parts_name),
                            );
                        }
                    }
                }

                for icon in [
                    &info.editor_upper_icon_name,
                    &info.editor_middle_icon_name,
                    &info.editor_lower_icon_name,
                ] {
                    if !icon.is_empty() && !icon.starts_with("MiiEditor_") {
                        check_png(
                            &mut report,
                            "hair_parts_attach_info",
                            Some(icon),
                            attach_icon.as_ref(),
                        );
                    }
                }
            }
        }

        problems.sort_by_key(|p| p.asset);
        problems
    }
}

fn texture_spec(bntx: &Bntx, name: &str) -> Option<TextureSpec> {
    let texture = bntx.textures.iter().find(|t| t.name == name)?;
    Some(TextureSpec {
        name: name.to_string(),
        width: texture.info.width,
        height: texture.info.height,
        format: texture.info.format.name().to_string(),
        encodable: can_encode(&texture.info),
    })
}

pub fn png_size(path: &Path) -> Result<(u32, u32)> {
    let bytes = std::fs::read(path).with_context(|| format!("reading '{}'", path.display()))?;

    anyhow::ensure!(
        bytes.len() >= 24 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") && &bytes[12..16] == b"IHDR",
        "'{}' is not a PNG",
        path.display()
    );

    let size = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    Ok((size(16), size(20)))
}

fn check_png(
    report: &mut impl FnMut(&'static str, String),
    field: &'static str,
    path: Option<&str>,
    expected: Option<&TextureSpec>,
) {
    let Some(path) = path else {
        return;
    };

    match png_size(Path::new(path)) {
        Ok((width, height)) => {
            if let Some(spec) = expected
                && (width, height) != (spec.width, spec.height)
            {
                report(
                    field,
                    format!(
                        "'{path}' is {width}x{height}, '{}' needs {}x{}",
                        spec.name, spec.width, spec.height
                    ),
                );
            }
        }
        Err(err) => report(field, format!("{err:#}")),
    }
}

pub fn bfres_models(path: &Path) -> Result<Vec<String>> {
    let rel = path.to_str().context("non UTF-8 path")?;
    let bfres = bfres_parse(&read_and_decompress(Path::new(""), rel)?)?;
    anyhow::ensure!(!bfres.models.names.is_empty(), "'{rel}' contains no models");

    Ok(bfres.models.names.clone())
}

pub fn resolve_models(
    entry: &PartsEntry,
    cat: &dyn CategoryDef,
    names: &[String],
) -> Result<Vec<ResolvedUnit>> {
    let pick = |requested: Option<&str>, auto: &dyn Fn(&str) -> bool| -> Result<Option<String>> {
        match requested {
            Some(name) => {
                anyhow::ensure!(
                    names.iter().any(|n| n == name),
                    "no model '{name}' (models: {})",
                    names.join(", ")
                );
                Ok(Some(name.to_string()))
            }
            None => Ok(names.iter().find(|n| auto(n)).cloned()),
        }
    };

    let mut units = Vec::new();
    if cat.asset_kind() == AssetKind::HairParts {
        let requested = &entry.hair_parts_model_unit;

        let slots: Vec<HairPartType> = if requested.is_empty() {
            HairPartType::KNOWN.to_vec()
        } else {
            requested.keys().copied().collect()
        };

        for slot in slots {
            let suffix = format!("_{}", slot.file_suffix());
            let unit = requested.get(&slot);

            let Some(model) = pick(unit.and_then(|u| u.fmdb.as_deref()), &|n| {
                n.ends_with(&suffix)
            })?
            else {
                anyhow::ensure!(unit.is_none(), "no model for slot {slot:?} (*{suffix})");
                continue;
            };

            units.push(ResolvedUnit {
                slot: UnitSlot::HairPart(slot),
                model,
                phcl: unit.and_then(|u| u.phcl.clone()),
                cloth_suffix: suffix,
            });
        }

        anyhow::ensure!(
            !units.is_empty(),
            "no models named after a hair part slot (e.g. *_UpperLeft)"
        );

        return Ok(units);
    }

    let is_hat = |n: &str| n.contains("Hat");
    let is_flip = |n: &str| n.ends_with("_Flip");
    let requested = |unit: &Option<crate::category::PartsModelUnitParam>| {
        unit.as_ref().and_then(|u| u.fmdb.clone())
    };

    let extras = [
        (UnitSlot::Hat, &entry.model_unit_for_hat, "_Hat"),
        (UnitSlot::Flip, &entry.flipped_model_unit, "_Flip"),
        (
            UnitSlot::FlipHat,
            &entry.flipped_model_unit_for_hat,
            "_HatFlip",
        ),
    ];

    for (slot, unit, suffix) in extras {
        let auto = |n: &str| match slot {
            UnitSlot::Hat => is_hat(n) && !is_flip(n),
            UnitSlot::Flip => is_flip(n) && !is_hat(n),
            _ => is_hat(n) && is_flip(n),
        };

        if let Some(model) = pick(requested(unit).as_deref(), &auto)? {
            units.push(ResolvedUnit {
                slot,
                model,
                phcl: unit.as_ref().and_then(|u| u.phcl.clone()),
                cloth_suffix: suffix.into(),
            });
        }
    }

    let main = names
        .iter()
        .find(|n| !units.iter().any(|u| &u.model == *n))
        .context("no model left for the main model unit")?;

    units.insert(
        0,
        ResolvedUnit {
            slot: UnitSlot::Main,
            model: main.clone(),
            phcl: entry.model_unit.phcl.clone(),
            cloth_suffix: String::new(),
        },
    );

    Ok(units)
}

pub(crate) fn part_row(doc: &Byml, file_name: &str) -> Result<Option<BTreeMap<String, Value>>> {
    let row_id = format!("Work/Mii/Parts/{file_name}.mii__Parts.gyml");
    Ok(byml_root_array(doc)?.iter().find_map(|row| match row {
        Value::Dict(map) if matches!(map.get("__RowId"), Some(Value::String(id)) if *id == row_id) => {
            Some(map.clone())
        }
        _ => None,
    }))
}

pub(crate) fn template_row(
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
