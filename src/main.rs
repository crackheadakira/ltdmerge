mod add;
mod merge;
mod rstbl;
mod util;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "ltdmerge")]
#[command(about = "Mod tool for Tomodachi Life: Living the Dream — create and merge head mods")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Merge N input mods into a single output mod, resolving collisions.
    Merge {
        /// Input mod directories (romfs layout). At least two required.
        #[arg(required = true, num_args = 2..)]
        mods: Vec<PathBuf>,

        /// Output directory for the merged mod.
        #[arg(short, long)]
        out: PathBuf,
    },

    /// Add a new additive head to a mod directory (or create one from scratch).
    ///
    /// Reads the existing files from --base (your romfs dump), adds a new
    /// Faceline entry, and writes the patched files to --out.
    Add {
        /// Path to your romfs dump (read-only, used as the base).
        #[arg(long)]
        base: PathBuf,

        /// Path to the .bfres model file.
        /// Example: MiiHead16.bfres.zs or MiiHead16.bfres.
        #[arg(long)]
        model: String,

        /// Path to your .png icon file (for MiiEditorIcon.bntx).
        /// Omit to skip icon injection.
        #[arg(long)]
        icon: Option<PathBuf>,

        /// Output directory for the generated mod (romfs layout).
        #[arg(short, long)]
        out: PathBuf,

        /// Where in the editor the new head appears.
        /// Defaults to appending after all existing entries.
        #[arg(long)]
        order_index: Option<usize>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Merge { mods, out } => merge::run(mods, out),
        Command::Add {
            base,
            model,
            icon,
            out,
            order_index,
        } => add::run(base, model, icon, out, order_index),
    }
}
