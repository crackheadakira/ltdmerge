use crate::category::{AssetKind, CategoryDef, PartsEntry, attach_icon_name};
use crate::types::CustomModNamespace;
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};

pub struct MiiMergeEngine {
    pub categories: Vec<Box<dyn CategoryDef>>,
    pub mods: Vec<CustomModNamespace>,
    /// Maps: `(CategoryName, ModIndex, OldIndex) -> NewIndex`
    pub allocation_map: BTreeMap<(String, usize, i32), i32>,
    pub reserved_indices: BTreeMap<String, BTreeSet<i32>>,
    pub attach_icon_renames: BTreeMap<usize, BTreeMap<String, String>>,
}

impl MiiMergeEngine {
    pub fn new(mods: Vec<CustomModNamespace>, categories: Vec<Box<dyn CategoryDef>>) -> Self {
        Self {
            mods,
            categories,
            allocation_map: BTreeMap::new(),
            reserved_indices: BTreeMap::new(),
            attach_icon_renames: BTreeMap::new(),
        }
    }

    pub fn reserve_indices(&mut self, indices: BTreeMap<String, BTreeSet<i32>>) {
        self.reserved_indices = indices;
    }

    pub fn compute_allocations(&mut self) -> Result<()> {
        self.allocation_map.clear();

        let mut groups: Vec<Vec<&dyn CategoryDef>> = Vec::new();
        for cat in &self.categories {
            match groups
                .iter_mut()
                .find(|g| g[0].internal_category_name() == cat.internal_category_name())
            {
                Some(group) => group.push(cat.as_ref()),
                None => groups.push(vec![cat.as_ref()]),
            }
        }

        for group in groups {
            let mut reserved = BTreeSet::new();

            for cat in &group {
                reserved.extend(
                    self.reserved_indices
                        .get(cat.category_name())
                        .into_iter()
                        .flatten(),
                );
            }

            let mut occupied = reserved.clone();
            let mut claims = Vec::new();

            for cat in &group {
                let name = cat.category_name();

                for custom_mod in &self.mods {
                    let mut indices = BTreeSet::new();

                    if let Some(data) = custom_mod.categories.get(name) {
                        for entry in &data.rstbl_entries {
                            if entry.is_ltdmerge_gen {
                                anyhow::ensure!(
                                    indices.insert(entry.parts_index),
                                    "duplicate marked {} index {} in mod {}",
                                    name,
                                    entry.parts_index,
                                    custom_mod.mod_index
                                );
                            }
                        }

                        indices.extend(data.pack_parts.keys().copied());
                    }

                    for index in indices {
                        anyhow::ensure!(index >= 0, "negative part index");
                        occupied.insert(index);
                        claims.push((name, custom_mod.mod_index, index));
                    }
                }
            }

            let vanilla_max = group
                .iter()
                .map(|c| c.vanilla_max_parts_index())
                .max()
                .unwrap_or_default();

            let mut claimed = reserved;
            let mut next = occupied
                .last()
                .copied()
                .unwrap_or(vanilla_max)
                .max(vanilla_max)
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("part index overflow"))?;

            for (name, mod_id, old) in claims {
                let assigned = if claimed.insert(old) {
                    old
                } else {
                    while occupied.contains(&next) || claimed.contains(&next) {
                        next = next
                            .checked_add(1)
                            .ok_or_else(|| anyhow::anyhow!("part index overflow"))?;
                    }

                    let assigned = next;
                    occupied.insert(assigned);
                    claimed.insert(assigned);
                    next = next
                        .checked_add(1)
                        .ok_or_else(|| anyhow::anyhow!("part index overflow"))?;
                    println!(
                        "[Collision] {name} | Mod {mod_id} Idx {old} reassigned to {assigned}"
                    );

                    assigned
                };

                self.allocation_map
                    .insert((name.to_string(), mod_id, old), assigned);
            }
        }

        Ok(())
    }

    pub fn resource_renames(&self, mod_id: usize, icons: bool) -> BTreeMap<String, String> {
        let mut result = BTreeMap::new();
        for cat in &self.categories {
            for ((category, owner, old), new) in &self.allocation_map {
                if category != cat.category_name() || *owner != mod_id || old == new {
                    continue;
                }

                if icons {
                    result.insert(
                        cat.editor_icon_name(*old as u32),
                        cat.editor_icon_name(*new as u32),
                    );

                    if let (Some(old), Some(new)) = (
                        cat.editor_mask_icon_name(*old as u32),
                        cat.editor_mask_icon_name(*new as u32),
                    ) {
                        result.insert(old, new);
                    }
                } else {
                    result.insert(
                        cat.internal_model_name(*old as u32),
                        cat.internal_model_name(*new as u32),
                    );
                }
            }
        }

        if icons && let Some(renames) = self.attach_icon_renames.get(&mod_id) {
            result.extend(renames.clone());
        }

        result
    }

    fn hair_parts_renames(&self, mod_id: usize) -> BTreeMap<String, String> {
        let mut result = BTreeMap::new();
        for cat in &self.categories {
            if cat.asset_kind() != AssetKind::HairParts {
                continue;
            }

            for ((category, owner, old), new) in &self.allocation_map {
                if category == cat.category_name() && *owner == mod_id && old != new {
                    result.insert(cat.part_name(*old as u32), cat.part_name(*new as u32));
                }
            }
        }

        result
    }

    pub fn execute_remap(&mut self) -> Result<()> {
        for i in 0..self.mods.len() {
            let mod_id = self.mods[i].mod_index;
            let model_names = self.resource_renames(mod_id, false);
            let icon_names = self.resource_renames(mod_id, true);
            let hair_parts = self.hair_parts_renames(mod_id);
            let mut attach_icons = BTreeMap::new();
            let custom_mod = &mut self.mods[i];

            for cat in &self.categories {
                let name = cat.category_name().to_string();
                if let Some(data) = custom_mod.categories.get_mut(&name) {
                    for entry in &mut data.rstbl_entries {
                        let old = entry.parts_index;
                        let new = self
                            .allocation_map
                            .get(&(name.clone(), mod_id, old))
                            .copied()
                            .unwrap_or(old);

                        let old_name = entry.file_name.clone();

                        entry.remap_to(
                            new,
                            cat.part_name(new as u32),
                            cat.row_id(new as u32),
                            &cat.internal_model_name(old as u32),
                            &cat.internal_model_name(new as u32),
                            cat.as_ref(),
                        );

                        remap_attach_info(entry, &old_name, &hair_parts, &mut attach_icons);
                        entry.validate()?;
                    }
                    let mut updated = BTreeMap::new();
                    for (old, mut part) in std::mem::take(&mut data.pack_parts) {
                        let new = self
                            .allocation_map
                            .get(&(name.clone(), mod_id, old))
                            .copied()
                            .unwrap_or(old);

                        let old_name = part.file_name.clone();
                        part.remap_to(
                            new,
                            cat.part_name(new as u32),
                            cat.row_id(new as u32),
                            &cat.internal_model_name(old as u32),
                            &cat.internal_model_name(new as u32),
                            cat.as_ref(),
                        );

                        remap_attach_info(&mut part, &old_name, &hair_parts, &mut attach_icons);
                        part.validate()?;
                        anyhow::ensure!(
                            updated.insert(new, part).is_none(),
                            "duplicate remapped part"
                        );
                    }

                    data.pack_parts = updated;
                }
            }

            let mut models = BTreeMap::new();
            for (name, bfres) in std::mem::take(&mut custom_mod.models) {
                let new = rename_resource(&name, &model_names);
                anyhow::ensure!(
                    models.insert(new, bfres).is_none(),
                    "duplicate remapped model"
                );
            }
            custom_mod.models = models;
            self.attach_icon_renames.insert(mod_id, attach_icons);
            if let Some(bntx) = &mut custom_mod.custom_icon {
                for texture in &mut bntx.textures {
                    if let Some(new) = icon_names.get(&texture.name) {
                        texture.name = new.clone();
                    }
                }
            }
        }
        Ok(())
    }
}

fn remap_attach_info(
    entry: &mut PartsEntry,
    old_hair_back: &str,
    hair_parts: &BTreeMap<String, String>,
    icons: &mut BTreeMap<String, String>,
) {
    let new_hair_back = entry.file_name.clone();

    for info in &mut entry.hair_parts_attach_info {
        let old_parts = info.hair_parts_name.clone();
        let new_parts = hair_parts
            .get(&old_parts)
            .cloned()
            .unwrap_or(old_parts.clone());

        for (icon, slot) in [
            (&mut info.editor_upper_icon_name, 'U'),
            (&mut info.editor_middle_icon_name, 'C'),
            (&mut info.editor_lower_icon_name, 'D'),
        ] {
            let old_icon = attach_icon_name(old_hair_back, &old_parts, slot);
            let new_icon = attach_icon_name(&new_hair_back, &new_parts, slot);

            if *icon == old_icon && old_icon != new_icon {
                icons.insert(old_icon, new_icon.clone());
                *icon = new_icon;
            } else if *icon == format!("MiiEditor_Face_{old_parts}_Uit") {
                *icon = format!("MiiEditor_Face_{new_parts}_Uit");
            }
        }

        info.hair_parts_name = new_parts;
    }
}

pub fn rename_resource(name: &str, renames: &BTreeMap<String, String>) -> String {
    for (old, new) in renames {
        if name == old {
            return new.clone();
        }

        if let Some(suffix) = name.strip_prefix(old).filter(|s| s.starts_with('_')) {
            return format!("{new}{suffix}");
        }
    }

    name.into()
}
