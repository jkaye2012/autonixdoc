use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, ValueEnum};

use crate::{
    mapping::{PathMapping, get_mapping},
    nixdoc::AutoNixdoc,
};

/// Externally supported mapping types that can be selected by end users.
///
/// Once a mapping is added here, it's part of the public API and will have
/// to be supported over time; take care!
#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum MappingType {
    /// Automatic mapping
    Auto,
}

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

    /// The path mapping strategy that should be used to generate documentation
    #[arg(short, long, value_enum, default_value_t = MappingType::Auto)]
    mapping: MappingType,
}

// TODO: implement configuration file, environment variables
// TODO: Implement another mapper to demonstrate how it works
// TODO: Initial documentation

impl Driver {
    pub fn run(self) -> Result<()> {
        println!("Hello");

        let mapping = get_mapping(self.mapping, &self.input_dir, &self.output_dir)?;
        // TODO: prefix extraction, can default to empty
        let autonixdoc = AutoNixdoc::new("", "", mapping);
        Self::run_in_path(&autonixdoc, &self.input_dir)
    }

    fn run_in_path<'a, M: PathMapping>(autonixdoc: &AutoNixdoc<'a, M>, path: &Path) -> Result<()> {
        // TODO: failure handling; don't have to abort immediately on failure if the user doesn't want to
        for entry in std::fs::read_dir(path)? {
            let path = entry?.path();

            if path.is_dir() {
                Self::run_in_path(&autonixdoc, &path)?;
            } else if let Some(ex) = path.extension()
                && ex.to_str() == Some("nix")
            // TODO: path identification strategy?
            {
                autonixdoc.execute(&path)?;
            } else {
                // TODO: logging strategy? Logging in general?
            }
        }

        Ok(())
    }
}
