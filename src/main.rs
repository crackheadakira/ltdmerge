use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use std::io::Read;
use std::path::PathBuf;

use ltdmerge::categories::{EarDef, EyeDef, FacelineDef, HairBackDef, HairFrontDef};
use ltdmerge::manifest::AddManifest;
use ltdmerge::registry::CategoryRegistry;

use ltdmerge::add;
use ltdmerge::merge;

#[derive(Parser)]
#[command(name = "ltdmerge")]
#[command(
    about = "A mod tool to streamline the process of creating new items in the Mii editor for Tomodachi Life Living The Dream."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Merge N input mods into a single output mod, resolving index collisions
    /// across all categories automatically.
    Merge {
        /// Input mod directories (romfs layout). At least two required.
        #[arg(required = true, num_args = 2..)]
        mods: Vec<PathBuf>,

        /// Output directory for the merged mod.
        #[arg(short, long)]
        out: PathBuf,
    },

    /// Add one or more custom assets to a mod directory from a JSON manifest.
    ///
    /// Pass a file path or `-` to read from stdin.
    Add {
        /// Path to your romfs dump (read-only base reference layout).
        #[arg(long)]
        base: PathBuf,

        /// Output directory for the generated mod content.
        #[arg(short, long)]
        out: PathBuf,

        /// Path to the JSON manifest file, or `-` to read from stdin.
        manifest: PathBuf,
    },
}

fn build_registry() -> CategoryRegistry {
    let mut registry = CategoryRegistry::new();
    registry.register(FacelineDef);
    registry.register(HairFrontDef);
    registry.register(EyeDef);
    registry.register(EarDef);
    registry.register(HairBackDef);
    registry
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Merge { mods, out } => {
            let registry = build_registry();
            let categories = registry.into_all();

            merge::run(mods, out, categories)
                .context("Failed to merge custom asset modifications")?;
        }

        Command::Add {
            base,
            out,
            manifest,
        } => {
            let json = read_manifest_source(&manifest)
                .with_context(|| format!("reading manifest '{}'", manifest.display()))?;

            let manifest = AddManifest::from_json(&json)?;

            if manifest.assets.is_empty() {
                bail!("manifest contains no assets");
            }

            let registry = build_registry();

            for spec in &manifest.assets {
                if registry.get(&spec.category).is_none() {
                    bail!(
                        "unknown category '{}', registered categories are: {}",
                        spec.category,
                        registry.known_names().join(", ")
                    );
                }
            }

            add::run(&base, &out, &manifest, &registry)
                .context("Failed to register and inject additive asset entries")?;
        }
    }

    Ok(())
}

fn read_manifest_source(path: &PathBuf) -> Result<String> {
    if path.as_os_str() == "-" {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("reading manifest from stdin")?;
        Ok(buf)
    } else {
        std::fs::read_to_string(path)
            .with_context(|| format!("reading manifest file '{}'", path.display()))
    }
}
