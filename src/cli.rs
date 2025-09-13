use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

/// Automatically generates nixdoc documentation for a library tree
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Driver {
    /// The directory containing the Nix library
    #[arg(short, long)]
    input_dir: PathBuf,

    /// The directory where generated documentation will be stored
    #[arg(short, long)]
    output_dir: PathBuf,
}

impl Driver {
    pub fn run(self) -> Result<()> {
        println!("Hello");
        Ok(())
    }
}
