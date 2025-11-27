use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use ignore::Walk;
use log::{LevelFilter, error, info};
use regex::Regex;

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
            _ => Err(format!("Unknown failure behavior: {}", s)),
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

    /// Regular expression pattern for identifying files to process
    #[arg(long)]
    regex_pattern: Option<String>,
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

/// Strategy for identifying which files should be processed for documentation.
#[derive(Debug, Clone)]
pub enum PathIdentification {
    /// Files ending in ".nix"
    NixExtension,
    /// Files matching a user-provided regular expression
    Regex(Regex),
}

impl Default for PathIdentification {
    fn default() -> Self {
        Self::NixExtension
    }
}

impl PathIdentification {
    /// Creates a PathIdentification strategy from an optional regex pattern.
    ///
    /// If pattern is provided, creates a Regex variant with the compiled regex.
    /// If pattern is None, creates the default Extension variant.
    fn from_pattern(pattern: Option<String>) -> Result<Self> {
        match pattern {
            Some(pattern) => {
                let regex = Regex::new(&pattern)
                    .with_context(|| format!("Invalid regex pattern: {}", pattern))?;
                Ok(Self::Regex(regex))
            }
            None => Ok(Self::NixExtension),
        }
    }

    /// Determines if a file should be processed based on the identification strategy.
    fn should_process(&self, path: &Path) -> bool {
        match self {
            Self::NixExtension => path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == "nix")
                .unwrap_or(false),
            Self::Regex(regex) => regex.is_match(&path.to_string_lossy()),
        }
    }
}

mod env_vars {
    pub const CONFIG: &'static str = "AUTONIXDOC_CONFIG";
    pub const ON_FAILURE: &'static str = "AUTONIXDOC_ON_FAILURE";
    pub const PREFIX: &'static str = "AUTONIXDOC_PREFIX";
    pub const ANCHOR_PREFIX: &'static str = "AUTONIXDOC_ANCHOR_PREFIX";
    pub const LOGGING_LEVEL: &'static str = "AUTONIXDOC_LOGGING_LEVEL";
    pub const REGEX_PATTERN: &'static str = "AUTONIXDOC_REGEX_PATTERN";
}

struct Behaviors {
    on_failure: FailureBehavior,
    path_identification: PathIdentification,
}

impl Behaviors {
    fn new(on_failure: Option<FailureBehavior>, regex_pattern: Option<String>) -> Result<Self> {
        Ok(Self {
            on_failure: on_failure.unwrap_or_default(),
            path_identification: PathIdentification::from_pattern(regex_pattern)?,
        })
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

        let regex_pattern = resolve_option(self.regex_pattern.clone(), env_vars::REGEX_PATTERN);
        let behaviors = Behaviors::new(failure_behavior, regex_pattern)?;

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

            if !path.is_dir() && behaviors.path_identification.should_process(&path) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_path_identification_extension_default() {
        let identification = PathIdentification::default();
        assert!(matches!(identification, PathIdentification::NixExtension));
    }

    #[test]
    fn test_path_identification_from_pattern_none() {
        let identification = PathIdentification::from_pattern(None).unwrap();
        assert!(matches!(identification, PathIdentification::NixExtension));
    }

    #[test]
    fn test_path_identification_from_pattern_some() {
        let identification = PathIdentification::from_pattern(Some(r"\.rs$".to_string())).unwrap();
        assert!(matches!(identification, PathIdentification::Regex(_)));
    }

    #[test]
    fn test_path_identification_from_pattern_invalid_regex() {
        let result = PathIdentification::from_pattern(Some("[".to_string()));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid regex pattern")
        );
    }

    #[test]
    fn test_path_identification_extension_should_process_nix() {
        let identification = PathIdentification::NixExtension;
        let path = PathBuf::from("/path/to/file.nix");
        assert!(identification.should_process(&path));
    }

    #[test]
    fn test_path_identification_extension_should_not_process_other() {
        let identification = PathIdentification::NixExtension;
        let path = PathBuf::from("/path/to/file.rs");
        assert!(!identification.should_process(&path));
    }

    #[test]
    fn test_path_identification_extension_should_not_process_no_extension() {
        let identification = PathIdentification::NixExtension;
        let path = PathBuf::from("/path/to/file");
        assert!(!identification.should_process(&path));
    }

    #[test]
    fn test_path_identification_regex_should_process_matching() {
        let regex = Regex::new(r"\.rs$").unwrap();
        let identification = PathIdentification::Regex(regex);
        let path = PathBuf::from("/path/to/file.rs");
        assert!(identification.should_process(&path));
    }

    #[test]
    fn test_path_identification_regex_should_not_process_non_matching() {
        let regex = Regex::new(r"\.rs$").unwrap();
        let identification = PathIdentification::Regex(regex);
        let path = PathBuf::from("/path/to/file.nix");
        assert!(!identification.should_process(&path));
    }

    #[test]
    fn test_path_identification_regex_complex_pattern() {
        let regex = Regex::new(r".*/(lib|src)/.*\.nix$").unwrap();
        let identification = PathIdentification::Regex(regex);

        let path1 = PathBuf::from("/project/lib/module.nix");
        let path2 = PathBuf::from("/project/src/utils.nix");
        let path3 = PathBuf::from("/project/docs/readme.nix");

        assert!(identification.should_process(&path1));
        assert!(identification.should_process(&path2));
        assert!(!identification.should_process(&path3));
    }

    #[test]
    fn test_behaviors_new_with_extension_default() {
        let behaviors = Behaviors::new(None, None).unwrap();
        assert_eq!(behaviors.on_failure, FailureBehavior::Log);
        assert!(matches!(
            behaviors.path_identification,
            PathIdentification::NixExtension
        ));
    }

    #[test]
    fn test_behaviors_new_with_regex_pattern() {
        let behaviors =
            Behaviors::new(Some(FailureBehavior::Abort), Some(r"\.rs$".to_string())).unwrap();
        assert_eq!(behaviors.on_failure, FailureBehavior::Abort);
        assert!(matches!(
            behaviors.path_identification,
            PathIdentification::Regex(_)
        ));
    }

    #[test]
    fn test_behaviors_new_with_invalid_regex() {
        let result = Behaviors::new(None, Some("[".to_string()));
        assert!(result.is_err());
    }
}
