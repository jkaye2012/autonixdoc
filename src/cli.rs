use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

/// Automatically generates nixdoc documentation for a library tree
///
/// By default (with no configuration file supplied), all Nix source files in INPUT_DIR will be
/// documented using nixdoc.
///
/// The resulting documentation will be created in OUTPUT_DIR with a directory structure mirroring
/// that of the input files one-to-one.
#[derive(Parser, Debug)]
#[command(version, long_about)]
pub struct Driver {
    /// The directory containing the Nix library
    #[arg(short, long)]
    input_dir: PathBuf,

    /// The directory where generated documentation will be stored
    #[arg(short, long)]
    output_dir: PathBuf,
}

// Next: implement configuration file, environment variables
// Wire up to AutoNixdoc functionality
// Implement another mapper to demonstrate how it works
// Initial documentation

impl Driver {
    pub fn run(self) -> Result<()> {
        println!("Hello");
        Ok(())
    }
}
