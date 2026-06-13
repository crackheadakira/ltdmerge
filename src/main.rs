mod add;
mod categories;
mod category;
mod engine;
mod merge;
mod rstbl;
mod types;
mod util;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::categories::{FacelineDef, HairFrontDef};
use crate::category::CategoryDef;

#[derive(Parser)]
#[command(name = "ltdmerge")]
#[command(
    about = "A mod tool to streamline the process of creating new items in the Mii editor for Tomodachi Life Living The Dream."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum AssetCategory {
    Faceline,
    HairFront,
}

impl AssetCategory {
    fn into_trait_object(self) -> Box<dyn CategoryDef> {
        match self {
            AssetCategory::Faceline => Box::new(FacelineDef),
            AssetCategory::HairFront => Box::new(HairFrontDef),
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Merge N input mods into a single output mod, resolving collisions across all categories automatically.
    Merge {
        /// Input mod directories (romfs layout). At least two required.
        #[arg(required = true, num_args = 2..)]
        mods: Vec<PathBuf>,

        /// Output directory for the merged mod.
        #[arg(short, long)]
        out: PathBuf,
    },

    /// Add a new custom asset to a mod directory (or create one from scratch).
    Add {
        /// Path to your romfs dump (read-only, used as the base reference layout).
        #[arg(long)]
        base: PathBuf,

        /// The category type of the asset you are adding.
        #[arg(short, long, value_enum)]
        category: AssetCategory,

        /// Path to the asset file.
        /// For Faceline/Hair: Path to the .bfres model.
        /// For Eyes/Mouths/Beards: Path to a loose .png or raw texture file.
        #[arg(long)]
        model: String,

        /// Path to your custom .png preview icon file (for MiiEditorIcon.bntx).
        /// Omit to skip icon injection and use a default fallback.
        #[arg(long)]
        icon: Option<PathBuf>,

        /// Output directory for the generated mod content.
        #[arg(short, long)]
        out: PathBuf,

        /// Custom sorting placement where the asset appears in the editor.
        /// Defaults to appending after all existing entries.
        #[arg(long)]
        order_index: Option<usize>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Merge { mods, out } => {
            let active_categories: Vec<Box<dyn CategoryDef>> = vec![
                Box::new(FacelineDef),
                Box::new(HairFrontDef),
                // Box::new(BeardDef),
            ];

            merge::run(mods, out, active_categories)
                .context("Failed to merge custom asset modifications")?;
        }
        Command::Add {
            base,
            category,
            model,
            icon,
            out,
            order_index,
        } => {
            let selected_cat = category.into_trait_object();

            add::run(base, selected_cat, model, icon, out, order_index)
                .context("Failed to register and inject additive asset entry")?;
        }
    }

    Ok(())
}
