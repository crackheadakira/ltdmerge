use crate::category::CategoryDef;
use std::collections::HashMap;

pub struct CategoryRegistry {
    entries: HashMap<String, Box<dyn CategoryDef>>,
}

impl CategoryRegistry {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn register(&mut self, cat: impl CategoryDef + 'static) {
        self.entries
            .insert(cat.internal_category_name().to_string(), Box::new(cat));
    }

    pub fn get(&self, name: &str) -> Option<&dyn CategoryDef> {
        self.entries.get(name).map(|b| b.as_ref())
    }

    pub fn into_all(self) -> Vec<Box<dyn CategoryDef>> {
        self.entries.into_values().collect()
    }

    pub fn all(&self) -> impl Iterator<Item = &dyn CategoryDef> {
        self.entries.values().map(|b| b.as_ref())
    }

    pub fn known_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.entries.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }
}
