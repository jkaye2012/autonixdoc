//! [Path mapping](PathMapping) abstraction.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Maps input paths (Nix files) to output paths (documentation markdown files).
///
/// Path mapping allows implementation of different strategies for documentation
/// structure.
pub trait PathMapping {
    fn resolve(&self, nix_path: &Path) -> Result<PathBuf>;
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
    pub fn new(source_base: &'a Path, dest_base: &'a Path) -> Self {
        AutoMapping {
            source_base,
            dest_base,
        }
    }
}

impl<'a> PathMapping for AutoMapping<'a> {
    fn resolve(&self, source_path: &Path) -> Result<PathBuf> {
        let source_dir = source_path
            .parent()
            .with_context(|| "source path had no parent")?;
        let relative_path = source_dir.strip_prefix(self.source_base).expect(
            "Source directory isn't a prefix of source path? Please report this, it's a bug",
        );

        let source_stem = source_path
            .file_stem()
            .with_context(|| "source path had no file name")?;

        Ok(self
            .dest_base
            .to_path_buf()
            .join(relative_path)
            .join(source_stem)
            .with_extension("md"))
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
        let result = mapping.resolve(&source_path).unwrap();
        let expected = PathBuf::from("/docs/lib/module.md");

        assert_eq!(result, expected);
    }

    #[test]
    fn test_auto_mapping_absolute_nested() {
        let source_path = PathBuf::from("/project/src/deep/nested/file.nix");
        let source_base = PathBuf::from("/project/src");
        let dest_base = PathBuf::from("/output");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&source_path).unwrap();
        let expected = PathBuf::from("/output/deep/nested/file.md");

        assert_eq!(result, expected);
    }

    #[test]
    fn test_auto_mapping_absolute_root_level() {
        let source_path = PathBuf::from("/src/default.nix");
        let source_base = PathBuf::from("/src");
        let dest_base = PathBuf::from("/docs");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&source_path).unwrap();
        let expected = PathBuf::from("/docs/default.md");

        assert_eq!(result, expected);
    }

    #[test]
    fn test_auto_mapping_relative_basic() {
        let source_path = PathBuf::from("src/lib/module.nix");
        let source_base = PathBuf::from("src");
        let dest_base = PathBuf::from("docs");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&source_path).unwrap();
        let expected = PathBuf::from("docs/lib/module.md");

        assert_eq!(result, expected);
    }

    #[test]
    fn test_auto_mapping_relative_nested() {
        let source_path = PathBuf::from("project/src/deep/nested/file.nix");
        let source_base = PathBuf::from("project/src");
        let dest_base = PathBuf::from("output");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&source_path).unwrap();
        let expected = PathBuf::from("output/deep/nested/file.md");

        assert_eq!(result, expected);
    }

    #[test]
    fn test_auto_mapping_relative_root_level() {
        let source_path = PathBuf::from("src/default.nix");
        let source_base = PathBuf::from("src");
        let dest_base = PathBuf::from("docs");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&source_path).unwrap();
        let expected = PathBuf::from("docs/default.md");

        assert_eq!(result, expected);
    }

    #[test]
    fn test_auto_mapping_mixed_absolute_relative() {
        let source_path = PathBuf::from("/absolute/src/file.nix");
        let source_base = PathBuf::from("/absolute/src");
        let dest_base = PathBuf::from("relative/docs");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&source_path).unwrap();
        let expected = PathBuf::from("relative/docs/file.md");

        assert_eq!(result, expected);
    }

    #[test]
    fn test_auto_mapping_no_parent_error() {
        let source_path = PathBuf::from("/");
        let source_base = PathBuf::from("/src");
        let dest_base = PathBuf::from("/docs");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&source_path);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "source path had no parent");
    }

    #[test]
    fn test_auto_mapping_no_file_stem_error() {
        let source_path = PathBuf::from("/src/..");
        let source_base = PathBuf::from("/src");
        let dest_base = PathBuf::from("/docs");

        let mapping = AutoMapping::new(&source_base, &dest_base);
        let result = mapping.resolve(&source_path);

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
        let result = mapping.resolve(&source_path).unwrap();
        assert_eq!(result, PathBuf::from("docs/example.md"));
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
        let _ = mapping.resolve(&source_path);
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
        let _ = mapping.resolve(&source_path);
    }
}
