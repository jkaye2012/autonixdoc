use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use ignore::Walk;
use log::{LevelFilter, error, info};

use crate::{
    mapping::{BaselineConfig, PathMapping, get_mapping},
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

/// How individual nixdoc generation failures should be handled.
#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum, Default, serde::Deserialize)]
pub enum FailureBehavior {
    /// Any individual failure should result in the generation process aborting immediately.
    Abort,
    /// Individual failures should not abort the generation process, but should print an error.
    #[default]
    Log,
    /// Individual failures should be ignored entirely.
    Skip,
}

/// A newtype wrapper around LevelFilter to provide From<String> implementation
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct LogLevel(pub LevelFilter);

impl From<LogLevel> for LevelFilter {
    fn from(log_level: LogLevel) -> Self {
        log_level.0
    }
}

impl std::str::FromStr for LogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let level_filter = match s.to_lowercase().as_str() {
            "error" => LevelFilter::Error,
            "warn" => LevelFilter::Warn,
            "info" => LevelFilter::Info,
            "debug" => LevelFilter::Debug,
            "trace" => LevelFilter::Trace,
            _ => {
                return Err(format!("Unknown logging level: {}", s));
            }
        };
        Ok(LogLevel(level_filter))
    }
}

impl std::str::FromStr for FailureBehavior {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "abort" => Ok(Self::Abort),
            "log" => Ok(Self::Log),
            "skip" => Ok(Self::Skip),
            _ => {
                Err(format!("Unknown failure behavior: {}", s))
            }
        }
    }
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

    /// The desired behavior upon encountering individual failures
    #[arg(short = 'f', long, value_enum)]
    on_failure: Option<FailureBehavior>,

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
    logging_level: Option<LogLevel>,

    /// Prefix for generated identifiers in the documentation
    #[arg(short, long)]
    prefix: Option<String>,

    /// Prefix for anchor links in the generated documentation
    #[arg(short = 'a', long)]
    anchor_prefix: Option<String>,
}

// TODO: Implement another mapper to demonstrate how it works
// TODO: Initial documentation

fn resolve_option<T: std::str::FromStr>(cli_value: Option<T>, env_key: &str) -> Option<T> {
    cli_value.or_else(|| std::env::var(env_key).ok().and_then(|s| s.parse().ok()))
}

/// Resolves configuration values with three-tier priority: CLI > environment > config file.
///
/// This function implements the priority system where CLI arguments have the highest priority,
/// followed by environment variables, and finally configuration file values.
///
/// # Arguments
///
/// * `cli_value` - Value from CLI arguments (highest priority)
/// * `env_key` - Environment variable key to check
/// * `config_value` - Value from configuration file (lowest priority)
fn resolve_with_config<T: std::str::FromStr + Clone>(
    cli_value: Option<T>,
    env_key: &str,
    config_value: Option<T>,
) -> Option<T> {
    cli_value
        .or_else(|| std::env::var(env_key).ok().and_then(|s| s.parse().ok()))
        .or(config_value)
}


mod env_vars {
    pub const CONFIG: &'static str = "AUTONIXDOC_CONFIG";
    pub const ON_FAILURE: &'static str = "AUTONIXDOC_ON_FAILURE";
    pub const PREFIX: &'static str = "AUTONIXDOC_PREFIX";
    pub const ANCHOR_PREFIX: &'static str = "AUTONIXDOC_ANCHOR_PREFIX";
    pub const LOGGING_LEVEL: &'static str = "AUTONIXDOC_LOGGING_LEVEL";
}

struct Behaviors {
    on_failure: FailureBehavior,
}

impl Behaviors {
    fn new(on_failure: Option<FailureBehavior>) -> Self {
        Self {
            on_failure: on_failure.unwrap_or_default(),
        }
    }
}

mod constants {
    pub const DEFAULT_CONFIG_PATH: &'static str = "autonixdoc.toml";
}

impl Driver {
    pub fn run(self) -> Result<()> {
        let mapping = get_mapping(self.mapping, &self.input_dir, &self.output_dir);
        let config = Self::resolve_config(
            &mapping,
            resolve_option(self.config.clone(), env_vars::CONFIG),
        )
        .with_context(|| "Failed to resolve configuration file")?;

        let failure_behavior = resolve_with_config(
            self.on_failure,
            env_vars::ON_FAILURE,
            config.failure_behavior(),
        );
        let behaviors = Behaviors::new(failure_behavior);

        let logging_level = resolve_with_config(
            self.logging_level,
            env_vars::LOGGING_LEVEL,
            config.logging_level(),
        );
        self.initialize_logging(logging_level);

        let prefix = resolve_with_config(self.prefix.clone(), env_vars::PREFIX, config.prefix())
            .unwrap_or_default();

        let anchor_prefix = resolve_with_config(
            self.anchor_prefix.clone(),
            env_vars::ANCHOR_PREFIX,
            config.anchor_prefix(),
        )
        .unwrap_or_default();

        let autonixdoc = AutoNixdoc::new(&prefix, &anchor_prefix, self.input_dir.clone(), mapping);
        self.run_in_path(&autonixdoc, &config, &behaviors, &self.input_dir)
    }

    fn initialize_logging(&self, logging_level: Option<LogLevel>) {
        if let Some(level) = logging_level {
            env_logger::builder().filter_level(level.into()).init();
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
        &self,
        autonixdoc: &AutoNixdoc<'a, M>,
        config: &M::Config,
        behaviors: &Behaviors,
        path: &Path,
    ) -> Result<()> {
        for entry in Walk::new(path) {
            let path = match entry {
                Ok(entry) => entry.into_path(),
                Err(e) => match behaviors.on_failure {
                    FailureBehavior::Abort => {
                        return Err(e).with_context(|| "Failed to list directory");
                    }
                    FailureBehavior::Log => {
                        error!("Failed to list directory: {}", e);
                        continue;
                    }
                    FailureBehavior::Skip => continue,
                },
            };

            if !path.is_dir()
                && let Some(ex) = path.extension()
                && ex.to_str() == Some("nix")
            // TODO: path identification strategy?
            {
                info!("Generating documentation for {}", path.display());
                let exec_result = autonixdoc.execute(config, &path);
                if let Err(e) = exec_result {
                    match behaviors.on_failure {
                        FailureBehavior::Abort => {
                            return Err(e).with_context(|| {
                                format!(
                                    "Documentation generation failed for file {}",
                                    path.display()
                                )
                            });
                        }
                        FailureBehavior::Log => {
                            error!(
                                "Failed to generate documentation for {}: {}",
                                path.display(),
                                e
                            );
                            continue;
                        }
                        FailureBehavior::Skip => continue,
                    }
                }
            } else {
                info!("Skipping uninteresting path {}", path.display());
            }
        }

        Ok(())
    }
}
