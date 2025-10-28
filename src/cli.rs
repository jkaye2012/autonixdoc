use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use ignore::Walk;
use log::{LevelFilter, info};

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

    /// The configuration file that should be used to customize mapping-dependent functionality
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// The level of logging to enable
    ///
    /// Possible values:
    /// - info: all messages
    /// - warn: warning and error messages only
    /// - error: error messages only
    ///
    /// [default: warn]
    #[arg(short, long, verbatim_doc_comment)]
    logging_level: Option<LevelFilter>,
}

// TODO: implement configuration file, environment variables
// TODO: Implement another mapper to demonstrate how it works
// TODO: Initial documentation

fn resolve_option<T: From<String>>(cli_value: Option<T>, env_key: &str) -> Option<T> {
    cli_value.or_else(|| std::env::var(env_key).map(Into::into).ok())
}

mod env_vars {
    pub const CONFIG: &'static str = "AUTONIXDOC_CONFIG";
    pub const FAILURE_MODE: &'static str = "AUTONIXDOC_FAILURE_MODE";
}

mod constants {
    pub const DEFAULT_CONFIG_PATH: &'static str = "autonixdoc.toml";
}

impl Driver {
    pub fn run(self) -> Result<()> {
        self.initialize_logging();

        let mapping = get_mapping(self.mapping, &self.input_dir, &self.output_dir);
        let config = Self::resolve_config(&mapping, resolve_option(self.config, env_vars::CONFIG))
            .with_context(|| "Failed to resolve configuration file")?;
        // TODO: prefix extraction, can default to empty
        let autonixdoc = AutoNixdoc::new("", "", mapping);
        Self::run_in_path(&autonixdoc, &config, &self.input_dir)
    }

    fn initialize_logging(&self) {
        if let Some(level) = self.logging_level {
            env_logger::builder().filter_level(level).init();
        } else {
            env_logger::init();
        }
    }

    fn resolve_config<M: PathMapping>(_mapping: &M, path: Option<PathBuf>) -> Result<M::Config> {
        let default_config = PathBuf::from(constants::DEFAULT_CONFIG_PATH);

        if let Some(path) = path {
            info!(
                "Attempting to load user-provided configuration at {}",
                path.display()
            );

            let config = std::fs::read_to_string(&path)
                .with_context(|| "Failed to read configuration file from user-provided path")?;
            Ok(toml::from_str(&config).with_context(
                || "Failed to parse user-provided configuration file as valid TOML",
            )?)
        } else if let Ok(exists) = std::fs::exists(&default_config)
            && exists
        {
            info!(
                "Attempting to load default configuration at {}",
                default_config.display()
            );

            let config = std::fs::read_to_string(&default_config).with_context(
                || "A configuration file exists at the default path, but cannot be read",
            )?;
            Ok(toml::from_str(&config)
                .with_context(|| "Failed to parse default configuration file as valid TOML")?)
        } else {
            info!("No configuration found, falling back to defaults");
            Ok(Default::default())
        }
    }

    fn run_in_path<'a, M: PathMapping>(
        autonixdoc: &AutoNixdoc<'a, M>,
        config: &M::Config,
        path: &Path,
    ) -> Result<()> {
        // TODO: failure handling; don't have to abort immediately on failure if the user doesn't want to
        for entry in Walk::new(path) {
            let path = entry
                .with_context(|| "Failed to list directory")?
                .into_path();

            if !path.is_dir()
                && let Some(ex) = path.extension()
                && ex.to_str() == Some("nix")
            // TODO: path identification strategy?
            {
                info!("Generating documentation for {}", path.display());
                autonixdoc.execute(config, &path)?;
            } else {
                info!("Skipping uninteresting path {}", path.display());
            }
        }

        Ok(())
    }
}
