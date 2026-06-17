use ltdmerge::categories::{EarDef, EyeDef, FacelineDef, HairFrontDef};
use ltdmerge::registry::CategoryRegistry;
use std::fs;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let mut registry = CategoryRegistry::new();
    registry.register(FacelineDef);
    registry.register(HairFrontDef);
    registry.register(EyeDef);
    registry.register(EarDef);

    let schema_dir = Path::new("schema");
    fs::create_dir_all(schema_dir)?;

    for cat in registry.all() {
        let schema = cat.json_schema();
        let json = serde_json::to_string_pretty(&schema)?;
        let path = schema_dir.join(format!("{}.json", cat.category_name()));
        fs::write(&path, json)?;
        println!("wrote {}", path.display());
    }

    Ok(())
}
