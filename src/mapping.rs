//! [Path mapping](PathMapping) abstraction.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, de::DeserializeOwned};

use crate::cli::{FailureBehavior, LogLevel, MappingType};

/// Baseline configuration that all PathMapping configurations should implement.
///
/// This trait provides optional fields for user-configurable values that can be
/// set via configuration files, environment variables, or CLI arguments. The
/// priority order is: CLI arguments > environment variables > configuration file values.
pub trait BaselineConfig {
    /// Returns the failure behavior configured in this configuration, if any.
    fn failure_behavior(&self) -> Option<FailureBehavior>;

    /// Returns the prefix configured in this configuration, if any.
    fn prefix(&self) -> Option<String>;

    /// Returns the anchor prefix configured in this configuration, if any.
    fn anchor_prefix(&self) -> Option<String>;

    /// Returns the logging level configured in this configuration, if any.
    fn logging_level(&self) -> Option<LogLevel>;
}

/// Actions that can be performed with a mapped path.
///
/// In most cases, the path action will describe how output documentation (markdown files)
/// should be stored on disk.
#[derive(Debug, PartialEq, Eq)]
pub enum PathAction {
    /// Documentation should be output to the mapped path
    OutputTo(PathBuf),
    /// The path should be skipped
    Skip,
}

/// Maps input paths (Nix files) to output [path actions](PathAction).
///
/// Path mapping allows implementation of different strategies for documentation
/// structure.
pub trait PathMapping {
    type Config: Default + DeserializeOwned + BaselineConfig;

    fn resolve(&self, config: &Self::Config, nix_path: &Path) -> Result<PathAction>;
}

/// Constructs a [PathMapping].
///
/// # Arguments
///
/// * `mapping_type` - The type of path mapping to create
/// * `source_base` - The base directory of the source files
/// * `dest_base` - The base directory for documentation output
pub fn get_mapping<'a>(
    mapping_type: MappingType,
    source_base: &'a Path,
    dest_base: &'a Path,
) -> impl PathMapping {
    match mapping_type {
        MappingType::Auto => AutoMapping {
            source_base,
            dest_base,
        },
    }
}

/// Mirrors source file paths to corresponding documentation paths.
///
/// This implementation transforms source paths by preserving the directory
/// structure relative to a base path and changing the file extension to ".md".
pub struct AutoMapping<'a> {
    /// Base directory of the source files
    source_base: &'a Path,
    /// Base directory for documentation output
    dest_base: &'a Path,
}

impl<'a> AutoMapping<'a> {
    /// Creates a new MirrorMapping instance.
    ///
    /// # Arguments
    ///
    /// * `source_base` - The base directory of the source tree
    /// * `dest_base` - The base directory for the documentation output
    #[cfg(test)]
    pub fn new(source_base: &'a Path, dest_base: &'a Path) -> Self {
        AutoMapping {
            source_base,
            dest_base,
        }
    }
}

#[derive(Default, Deserialize)]
pub struct AutoMappingConfig {
    /// Paths to ignore during documentation generation
    pub ignore_paths: HashSet<PathBuf>,
    /// Failure behavior configuration
    pub failure_behavior: Option<FailureBehavior>,
    /// Prefix for generated identifiers
    pub prefix: Option<String>,
    /// Prefix for anchor links
    pub anchor_prefix: Option<String>,
    /// Logging level configuration as string (info, warn, error)
    pub logging_level: Option<String>,
}

impl BaselineConfig for AutoMappingConfig {
    fn failure_behavior(&self) -> Option<FailureBehavior> {
        self.failure_behavior
    }

    fn prefix(&self) -> Option<String> {
        self.prefix.clone()
    }

    fn anchor_prefix(&self) -> Option<String> {
        self.anchor_prefix.clone()
    }

    fn logging_level(&self) -> Option<LogLevel> {
        self.logging_level.as_ref().and_then(|s| s.parse().ok())
    }
}

impl<'a> PathMapping for AutoMapping<'a> {
    type Config = AutoMappingConfig;

    fn resolve(&self, config: &Self::Config, source_path: &Path) -> Result<PathAction> {
        if config.ignore_paths.contains(source_path) {
            return Ok(PathAction::Skip);
        }

        let source_dir = source_path
            .parent()
            .with_context(|| "source path had no parent")?;
        let relative_path = source_dir.strip_prefix(self.source_base).expect(
            "Source directory isn't a prefix of source path? Please report this, it's a bug",
        );

        let source_stem = source_path
            .file_stem()
            .with_context(|| "source path had no file name")?;

        Ok(PathAction::OutputTo(
            self.dest_base
                .to_path_buf()
                .join(relative_path)
                .join(source_stem)
                .with_extension("md"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_auto_mapping_absolute_basic() {
        let source_path = PathBuf::from("/src/lib/module.nix");
        let source_base = PathBuf::from("/src");
        let dest_base = PathBuf::from("/docs");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&Default::default(), &source_path).unwrap();
        let expected = PathBuf::from("/docs/lib/module.md");

        assert_eq!(result, PathAction::OutputTo(expected));
    }

    #[test]
    fn test_auto_mapping_absolute_nested() {
        let source_path = PathBuf::from("/project/src/deep/nested/file.nix");
        let source_base = PathBuf::from("/project/src");
        let dest_base = PathBuf::from("/output");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&Default::default(), &source_path).unwrap();
        let expected = PathBuf::from("/output/deep/nested/file.md");

        assert_eq!(result, PathAction::OutputTo(expected));
    }

    #[test]
    fn test_auto_mapping_absolute_root_level() {
        let source_path = PathBuf::from("/src/default.nix");
        let source_base = PathBuf::from("/src");
        let dest_base = PathBuf::from("/docs");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&Default::default(), &source_path).unwrap();
        let expected = PathBuf::from("/docs/default.md");

        assert_eq!(result, PathAction::OutputTo(expected));
    }

    #[test]
    fn test_auto_mapping_relative_basic() {
        let source_path = PathBuf::from("src/lib/module.nix");
        let source_base = PathBuf::from("src");
        let dest_base = PathBuf::from("docs");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&Default::default(), &source_path).unwrap();
        let expected = PathBuf::from("docs/lib/module.md");

        assert_eq!(result, PathAction::OutputTo(expected));
    }

    #[test]
    fn test_auto_mapping_relative_nested() {
        let source_path = PathBuf::from("project/src/deep/nested/file.nix");
        let source_base = PathBuf::from("project/src");
        let dest_base = PathBuf::from("output");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&Default::default(), &source_path).unwrap();
        let expected = PathBuf::from("output/deep/nested/file.md");

        assert_eq!(result, PathAction::OutputTo(expected));
    }

    #[test]
    fn test_auto_mapping_relative_root_level() {
        let source_path = PathBuf::from("src/default.nix");
        let source_base = PathBuf::from("src");
        let dest_base = PathBuf::from("docs");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&Default::default(), &source_path).unwrap();
        let expected = PathBuf::from("docs/default.md");

        assert_eq!(result, PathAction::OutputTo(expected));
    }

    #[test]
    fn test_auto_mapping_mixed_absolute_relative() {
        let source_path = PathBuf::from("/absolute/src/file.nix");
        let source_base = PathBuf::from("/absolute/src");
        let dest_base = PathBuf::from("relative/docs");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&Default::default(), &source_path).unwrap();
        let expected = PathBuf::from("relative/docs/file.md");

        assert_eq!(result, PathAction::OutputTo(expected));
    }

    #[test]
    fn test_auto_mapping_no_parent_error() {
        let source_path = PathBuf::from("/");
        let source_base = PathBuf::from("/src");
        let dest_base = PathBuf::from("/docs");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&Default::default(), &source_path);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "source path had no parent");
    }

    #[test]
    fn test_auto_mapping_no_file_stem_error() {
        let source_path = PathBuf::from("/src/..");
        let source_base = PathBuf::from("/src");
        let dest_base = PathBuf::from("/docs");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&Default::default(), &source_path);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "source path had no file name"
        );
    }

    #[test]
    fn test_auto_mapping_current_directory() {
        let source_path = PathBuf::from("./example.nix");
        let source_base = PathBuf::from(".");
        let dest_base = PathBuf::from("docs/");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&Default::default(), &source_path).unwrap();
        assert_eq!(
            result,
            PathAction::OutputTo(PathBuf::from("docs/example.md"))
        );
    }

    #[test]
    #[should_panic(
        expected = "Source directory isn't a prefix of source path? Please report this, it's a bug"
    )]
    fn test_auto_mapping_invalid_prefix_panic() {
        let source_path = PathBuf::from("/other/lib/module.nix");
        let source_base = PathBuf::from("/src");
        let dest_base = PathBuf::from("/docs");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let _ = mapping.resolve(&Default::default(), &source_path);
    }

    #[test]
    #[should_panic(
        expected = "Source directory isn't a prefix of source path? Please report this, it's a bug"
    )]
    fn test_auto_mapping_relative_invalid_prefix_panic() {
        let source_path = PathBuf::from("other/lib/module.nix");
        let source_base = PathBuf::from("src");
        let dest_base = PathBuf::from("docs");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let _ = mapping.resolve(&Default::default(), &source_path);
    }

    #[test]
    fn test_ignore_paths_single_file() {
        let source_path = PathBuf::from("/src/lib/module.nix");
        let source_base = PathBuf::from("/src");
        let dest_base = PathBuf::from("/docs");

        let mut config = AutoMappingConfig::default();
        config.ignore_paths.insert(source_path.clone());

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&config, &source_path).unwrap();

        assert_eq!(result, PathAction::Skip);
    }

    #[test]
    fn test_ignore_paths_multiple_files() {
        let source_path1 = PathBuf::from("/src/lib/module1.nix");
        let source_path2 = PathBuf::from("/src/lib/module2.nix");
        let source_path3 = PathBuf::from("/src/lib/module3.nix");
        let source_base = PathBuf::from("/src");
        let dest_base = PathBuf::from("/docs");

        let mut config = AutoMappingConfig::default();
        config.ignore_paths.insert(source_path1.clone());
        config.ignore_paths.insert(source_path3.clone());

        let mapping = AutoMapping::new(&source_base, &dest_base);

        let result1 = mapping.resolve(&config, &source_path1).unwrap();
        assert_eq!(result1, PathAction::Skip);

        let result2 = mapping.resolve(&config, &source_path2).unwrap();
        assert_eq!(
            result2,
            PathAction::OutputTo(PathBuf::from("/docs/lib/module2.md"))
        );

        let result3 = mapping.resolve(&config, &source_path3).unwrap();
        assert_eq!(result3, PathAction::Skip);
    }

    #[test]
    fn test_ignore_paths_relative_paths() {
        let source_path = PathBuf::from("src/lib/ignored.nix");
        let source_base = PathBuf::from("src");
        let dest_base = PathBuf::from("docs");

        let mut config = AutoMappingConfig::default();
        config.ignore_paths.insert(source_path.clone());

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&config, &source_path).unwrap();

        assert_eq!(result, PathAction::Skip);
    }

    #[test]
    fn test_ignore_paths_not_ignored() {
        let ignored_path = PathBuf::from("/src/lib/ignored.nix");
        let normal_path = PathBuf::from("/src/lib/normal.nix");
        let source_base = PathBuf::from("/src");
        let dest_base = PathBuf::from("/docs");

        let mut config = AutoMappingConfig::default();
        config.ignore_paths.insert(ignored_path);

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&config, &normal_path).unwrap();
        let expected = PathBuf::from("/docs/lib/normal.md");

        assert_eq!(result, PathAction::OutputTo(expected));
    }

    #[test]
    fn test_ignore_paths_nested_directories() {
        let ignored_path = PathBuf::from("/project/src/deep/nested/ignored.nix");
        let normal_path = PathBuf::from("/project/src/deep/nested/normal.nix");
        let source_base = PathBuf::from("/project/src");
        let dest_base = PathBuf::from("/output");

        let mut config = AutoMappingConfig::default();
        config.ignore_paths.insert(ignored_path.clone());

        let mapping = AutoMapping::new(&source_base, &dest_base);

        let ignored_result = mapping.resolve(&config, &ignored_path).unwrap();
        assert_eq!(ignored_result, PathAction::Skip);

        let normal_result = mapping.resolve(&config, &normal_path).unwrap();
        assert_eq!(
            normal_result,
            PathAction::OutputTo(PathBuf::from("/output/deep/nested/normal.md"))
        );
    }

    #[test]
    fn test_baseline_config_default_values() {
        let config = AutoMappingConfig::default();

        assert_eq!(config.failure_behavior(), None);
        assert_eq!(config.prefix(), None);
        assert_eq!(config.anchor_prefix(), None);
        assert_eq!(config.logging_level(), None);
    }

    #[test]
    fn test_baseline_config_with_values() {
        let mut config = AutoMappingConfig::default();
        config.failure_behavior = Some(FailureBehavior::Abort);
        config.prefix = Some("test-prefix".to_string());
        config.anchor_prefix = Some("test-anchor".to_string());
        config.logging_level = Some("info".to_string());

        assert_eq!(config.failure_behavior(), Some(FailureBehavior::Abort));
        assert_eq!(config.prefix(), Some("test-prefix".to_string()));
        assert_eq!(config.anchor_prefix(), Some("test-anchor".to_string()));
        assert_eq!(config.logging_level(), Some(LogLevel(log::LevelFilter::Info)));
    }

    #[test]
    fn test_baseline_config_logging_level_parsing() {
        let test_cases = vec![
            ("error", LogLevel(log::LevelFilter::Error)),
            ("warn", LogLevel(log::LevelFilter::Warn)),
            ("info", LogLevel(log::LevelFilter::Info)),
            ("debug", LogLevel(log::LevelFilter::Debug)),
            ("trace", LogLevel(log::LevelFilter::Trace)),
            ("ERROR", LogLevel(log::LevelFilter::Error)),
            ("WARN", LogLevel(log::LevelFilter::Warn)),
        ];

        for (input, expected) in test_cases {
            let mut config = AutoMappingConfig::default();
            config.logging_level = Some(input.to_string());

            assert_eq!(
                config.logging_level(),
                Some(expected),
                "Failed for input: {}",
                input
            );
        }
    }

    #[test]
    fn test_baseline_config_none_logging_level() {
        let config = AutoMappingConfig {
            ignore_paths: HashSet::new(),
            failure_behavior: None,
            prefix: None,
            anchor_prefix: None,
            logging_level: None,
        };

        assert_eq!(config.logging_level(), None);
    }
}
