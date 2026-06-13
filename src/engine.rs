use crate::{category::CategoryDef, types::CustomModNamespace};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};

pub struct MiiMergeEngine {
    pub categories: Vec<Box<dyn CategoryDef>>,
    pub mods: Vec<CustomModNamespace>,
    /// Maps: `(CategoryName, ModIndex, OldIndex) -> NewIndex`
    pub allocation_map: BTreeMap<(String, usize, i32), i32>,
}

impl MiiMergeEngine {
    pub fn new(mods: Vec<CustomModNamespace>, categories: Vec<Box<dyn CategoryDef>>) -> Self {
        Self {
            mods,
            categories,
            allocation_map: BTreeMap::new(),
        }
    }

    pub fn compute_allocations(&mut self) -> Result<()> {
        for cat in &self.categories {
            let cat_name = cat.category_name().to_string();
            let mut next_free_index = cat.vanilla_max_parts_index() + 1;
            let mut claimed_indices = BTreeMap::new();

            for custom_mod in &self.mods {
                let mod_idx = custom_mod.mod_index;
                let mut unique_mod_claims = BTreeSet::new();

                if let Some(data) = custom_mod.categories.get(&cat_name) {
                    for entry in &data.rstbl_entries {
                        unique_mod_claims.insert(entry.parts_index);
                    }
                }

                for old_idx in unique_mod_claims {
                    while next_free_index <= old_idx
                        || claimed_indices.contains_key(&next_free_index)
                    {
                        next_free_index += 1;
                    }

                    if let Some(&owner) = claimed_indices.get(&old_idx) {
                        let assigned = next_free_index;
                        next_free_index += 1;
                        println!(
                            "[Collision] {cat_name} | Mod {mod_idx} Idx {old_idx} reassigned to {assigned} (Conflict with Mod {owner})"
                        );

                        self.allocation_map
                            .insert((cat_name.clone(), mod_idx, old_idx), assigned);
                    } else {
                        claimed_indices.insert(old_idx, mod_idx);
                        self.allocation_map
                            .insert((cat_name.clone(), mod_idx, old_idx), old_idx);
                    }
                }
            }
        }

        Ok(())
    }

    pub fn execute_remap(&mut self) -> Result<()> {
        for cat in &self.categories {
            let cat_name = cat.category_name().to_string();

            for custom_mod in &mut self.mods {
                let mod_idx = custom_mod.mod_index;

                if let Some(data) = custom_mod.categories.get_mut(&cat_name) {
                    for entry in &mut data.rstbl_entries {
                        let old_idx = entry.parts_index;
                        if let Some(&new_idx) =
                            self.allocation_map
                                .get(&(cat_name.clone(), mod_idx, old_idx))
                        {
                            let old_token = cat.internal_model_name(old_idx as u32);
                            let new_token = cat.internal_model_name(new_idx as u32);

                            entry.remap_to(
                                new_idx,
                                cat.part_name(new_idx as u32),
                                cat.row_id(new_idx as u32),
                                &old_token,
                                &new_token,
                                cat.as_ref(),
                            );
                        }
                    }

                    let mut updated_pack = BTreeMap::new();
                    for (old_idx, mut part) in std::mem::take(&mut data.pack_parts) {
                        if let Some(&new_idx) =
                            self.allocation_map
                                .get(&(cat_name.clone(), mod_idx, old_idx))
                        {
                            let old_token = cat.internal_model_name(old_idx as u32);
                            let new_token = cat.internal_model_name(new_idx as u32);

                            part.remap_to(
                                new_idx,
                                cat.part_name(new_idx as u32),
                                cat.row_id(new_idx as u32),
                                &old_token,
                                &new_token,
                                cat.as_ref(),
                            );
                            updated_pack.insert(new_idx, part);
                        } else {
                            updated_pack.insert(old_idx, part);
                        }
                    }
                    data.pack_parts = updated_pack;
                }

                let mut updated_models = BTreeMap::new();
                for (old_name, mut bfres) in std::mem::take(&mut custom_mod.models) {
                    let num_str: String = old_name.chars().filter(|c| c.is_ascii_digit()).collect();
                    if let Ok(old_idx) = num_str.parse::<i32>() {
                        if let Some(&new_idx) =
                            self.allocation_map
                                .get(&(cat_name.clone(), mod_idx, old_idx))
                        {
                            let new_name = cat.internal_model_name(new_idx as u32);
                            bfres.name = new_name.clone();
                            updated_models.insert(new_name, bfres);
                            continue;
                        }
                    }
                    updated_models.insert(old_name, bfres);
                }

                custom_mod.models.extend(updated_models);

                if let Some(ref mut bntx) = custom_mod.custom_icon {
                    for tex in &mut bntx.textures {
                        let num_str: String =
                            tex.name.chars().filter(|c| c.is_ascii_digit()).collect();
                        if let Ok(old_idx) = num_str.parse::<i32>() {
                            if let Some(&new_idx) =
                                self.allocation_map
                                    .get(&(cat_name.clone(), mod_idx, old_idx))
                            {
                                if new_idx != old_idx {
                                    let old_token = cat.internal_model_name(old_idx as u32);
                                    let new_token = cat.internal_model_name(new_idx as u32);
                                    tex.name = tex.name.replace(&old_token, &new_token);
                                }
                            }
                        }
                    }
                }
            }
        }

        for custom_mod in &mut self.mods {
            let mod_idx = custom_mod.mod_index;
            for (cat_name, idx) in &mut custom_mod.custom_order_indices {
                let old_val = *idx as i32;
                if let Some(&new_idx) =
                    self.allocation_map
                        .get(&(cat_name.clone(), mod_idx, old_val))
                {
                    *idx = new_idx;
                }
            }
        }

        Ok(())
    }
}
