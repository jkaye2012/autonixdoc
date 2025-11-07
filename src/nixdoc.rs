//! Handlers for invocation of external `nixdoc` commands.

use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, anyhow};
use typed_builder::TypedBuilder;

use crate::mapping::{PathAction, PathMapping};

/// Builder for creating nixdoc commands.
///
/// This struct encapsulates the parameters needed to build a nixdoc command.
/// Use the builder pattern to construct instances and convert them to executable commands.
#[derive(TypedBuilder)]
struct Nixdoc<'a> {
    /// The category name for the documentation
    category: &'a str,
    /// Description text for the documentation
    description: &'a str,
    /// Path to the source file to document
    file: &'a str,
    /// Optional prefix for generated identifiers
    #[builder(default, setter(strip_option))]
    prefix: Option<&'a str>,
    /// Optional prefix for anchor links
    #[builder(default, setter(strip_option))]
    anchor_prefix: Option<&'a str>,
}

impl<'a> Into<Command> for Nixdoc<'a> {
    fn into(self) -> Command {
        let mut command = Command::new("nixdoc");
        command
            .arg("--category")
            .arg(self.category)
            .arg("--description")
            .arg(self.description)
            .arg("--file")
            .arg(self.file);
        if let Some(prefix) = self.prefix {
            command.arg("--prefix").arg(prefix);
        }
        if let Some(anchor) = self.anchor_prefix {
            command.arg("--anchor-prefix").arg(anchor);
        }

        command
    }
}

impl<'a> Nixdoc<'a> {
    /// Converts this Nixdoc instance into a Command ready for execution.
    ///
    /// This is a convenience function that delegates to the `Into<Command>` trait.
    pub fn into_command(self) -> Command {
        self.into()
    }
}

/// Automated nixdoc documentation generator.
///
/// This struct provides high-level automation for generating nixdoc documentation
/// from source files. It handles the complete workflow from reading source files
/// to generating markdown documentation using a configurable path mapping strategy.
pub struct AutoNixdoc<'a, M: PathMapping> {
    /// Prefix for generated identifiers
    prefix: &'a str,
    /// Prefix for anchor links in documentation
    anchor_prefix: &'a str,
    /// Path mapping strategy for determining output locations
    mapper: M,
    /// Input directory root for computing relative paths
    input_dir: PathBuf,
}

impl<'a, M: PathMapping> AutoNixdoc<'a, M> {
    /// Creates a new AutoNixdoc instance.
    ///
    /// # Arguments
    ///
    /// * `prefix` - Prefix for generated identifiers in the documentation
    /// * `anchor_prefix` - Prefix for anchor links in the generated documentation
    /// * `input_dir` - Input directory root for computing relative paths in categories
    /// * `mapper` - Path mapping strategy for determining output file locations
    pub fn new(prefix: &'a str, anchor_prefix: &'a str, input_dir: PathBuf, mapper: M) -> Self {
        AutoNixdoc {
            prefix: prefix.into(),
            anchor_prefix: anchor_prefix.into(),
            mapper,
            input_dir,
        }
    }

    /// Generates documentation for a single source file.
    ///
    /// This function processes a source file and generates corresponding markdown
    /// documentation using nixdoc. The output location is determined by the
    /// configured path mapping strategy.
    ///
    /// Note that depending on the behavior of the mapping strategy, it's possible
    /// for this function to return successfully without generation output documentation.
    /// This would be the case if e.g. a mapping strategy decides that a specific
    /// source file should be ignored.
    ///
    /// # Arguments
    ///
    /// * `path_ref` - Path to the source file to document
    ///
    /// # Errors
    ///
    /// Returns an error if:
    ///
    /// - The source path contains invalid Unicode
    /// - The path mapping fails
    /// - The source file cannot be read
    /// - The output directory cannot be created
    /// - The nixdoc command fails
    pub fn execute<P: AsRef<Path>>(&self, config: &M::Config, path_ref: P) -> Result<()> {
        let path = path_ref.as_ref();

        let path_action = self
            .mapper
            .resolve(config, path)
            .with_context(|| "path mapping failed")?;

        match path_action {
            PathAction::Skip => Ok(()),
            PathAction::OutputTo(dest_path) => self.output_to(path, dest_path),
        }
    }

    fn extract_category(&self, path: &Path) -> Result<String> {
        let relative_path = path
            .strip_prefix(&self.input_dir)
            .with_context(|| "source path is not within input directory")?;

        let file_stem = relative_path
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .with_context(|| "source path had no file name")?;

        let parent_components: Vec<&str> = relative_path
            .parent()
            .map(|p| {
                p.components()
                    .filter_map(|c| {
                        if let std::path::Component::Normal(os_str) = c {
                            os_str.to_str()
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let category = if parent_components.is_empty() {
            file_stem.to_string()
        } else {
            format!("{}.{}", parent_components.join("."), file_stem)
        };

        Ok(category)
    }

    fn output_to(&self, path: &Path, dest_path: PathBuf) -> Result<()> {
        let path_str = path
            .to_str()
            .with_context(|| "source path was not valid unicode")?;

        let category = self.extract_category(path)?;

        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(&parent).with_context(|| {
                format!(
                    "Failed to create documentation directory: {}",
                    parent.display()
                )
            })?;
        }

        let dest_file = File::create(&dest_path)
            .with_context(|| format!("Failed to create output file: {}", dest_path.display()))?;

        let file = File::open(path)?;
        let reader = BufReader::new(file);
        // TODO: description extraction strategy?
        let desc = reader
            .lines()
            .nth(1)
            .transpose()
            .with_context(|| format!("Failed to read input file: {}", path_str))?
            .unwrap_or_default();

        let nixdoc = Nixdoc::builder()
            .file(path_str)
            .category(&category)
            .description(&desc)
            .prefix(&self.prefix)
            .anchor_prefix(&self.anchor_prefix)
            .build();

        let output = nixdoc
            .into_command()
            .stdout(Stdio::from(dest_file))
            .output()
            .with_context(|| "nixdoc command execution failed")?;

        if output.status.success() {
            Ok(())
        } else {
            Err(anyhow!(
                "nixdoc command error: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, anyhow};
    use std::{
        ffi::OsStr, fs, os::unix::ffi::OsStrExt, os::unix::fs::PermissionsExt, path::PathBuf,
    };
    use tempfile::TempDir;

    use super::*;
    use crate::mapping::{AutoMapping, PathMapping};

    /// Test utility for setting up temporary directories
    fn setup_test_dirs() -> (TempDir, PathBuf, PathBuf) {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path();
        let input_dir = temp_path.join("input");
        let output_dir = temp_path.join("output");
        fs::create_dir_all(&input_dir).unwrap();
        (temp_dir, input_dir, output_dir)
    }

    #[test]
    fn test_nixdoc_execute_success() {
        const TEST_NIX_CONTENT: &str = include_str!("../resources/test-lib.nix");

        let (_temp_dir, input_dir, output_dir) = setup_test_dirs();

        let test_nix_file = input_dir.join("test-lib.nix");
        fs::write(&test_nix_file, TEST_NIX_CONTENT).unwrap();

        let mapping = AutoMapping::new(&input_dir, &output_dir);
        let nixdoc = AutoNixdoc::new("lib", "lib-", input_dir.clone(), mapping);

        let result = nixdoc.execute(&Default::default(), &test_nix_file);

        match result {
            Ok(()) => {
                let expected_output = output_dir.join("test-lib.md");
                assert!(expected_output.exists(), "Output file should be created");

                let content = fs::read_to_string(&expected_output).unwrap();
                assert!(
                    !content.is_empty(),
                    "Output file should contain documentation"
                );
                assert!(
                    content.contains("Utility functions"),
                    "Output file should contain module description"
                );
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_nixdoc_execute_nonexistent_file() {
        let (_temp_dir, input_dir, output_dir) = setup_test_dirs();

        let nonexistent_file = input_dir.join("nonexistent.nix");

        let mapping = AutoMapping::new(&input_dir, &output_dir);
        let nixdoc = AutoNixdoc::new("lib", "lib-", input_dir.clone(), mapping);
        let result = nixdoc.execute(&Default::default(), &nonexistent_file);

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("No such file"));
    }

    #[test]
    fn test_nixdoc_execute_invalid_unicode_path() {
        let (_temp_dir, input_dir, output_dir) = setup_test_dirs();

        // Create a path with invalid UTF-8
        let invalid_utf8 = OsStr::from_bytes(&[0x66, 0x6f, 0x6f, 0x80, 0x2e, 0x6e, 0x69, 0x78]); // "foo<invalid>.nix"
        let invalid_file = input_dir.join(invalid_utf8);

        let mapping = AutoMapping::new(&input_dir, &output_dir);
        let nixdoc = AutoNixdoc::new("lib", "lib-", input_dir.clone(), mapping);
        let result = nixdoc.execute(&Default::default(), &invalid_file);

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("source path was not valid unicode"));
    }

    #[test]
    fn test_nixdoc_execute_read_only_output_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path();
        let input_dir = temp_path.join("input");
        let output_dir = temp_path.join("readonly_output");
        fs::create_dir_all(&input_dir).unwrap();
        fs::create_dir_all(&output_dir).unwrap();

        // Make output directory read-only
        let mut perms = fs::metadata(&output_dir).unwrap().permissions();
        perms.set_mode(0o444);
        fs::set_permissions(&output_dir, perms).unwrap();

        let test_nix_file = input_dir.join("test.nix");
        fs::write(&test_nix_file, "# Test file\n# Description\n{ lib }: {}").unwrap();

        let mapping = AutoMapping::new(&input_dir, &output_dir);
        let nixdoc = AutoNixdoc::new("lib", "lib-", input_dir.clone(), mapping);
        let result = nixdoc.execute(&Default::default(), &test_nix_file);

        // Restore permissions for cleanup
        let mut perms = fs::metadata(&output_dir).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&output_dir, perms).unwrap();

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("Failed to create output file")
                || error_msg.contains("Permission denied")
        );
    }

    #[test]
    fn test_nixdoc_execute_empty_file() {
        let (_temp_dir, input_dir, output_dir) = setup_test_dirs();

        let empty_file = input_dir.join("empty.nix");
        fs::write(&empty_file, "").unwrap();

        let mapping = AutoMapping::new(&input_dir, &output_dir);
        let nixdoc = AutoNixdoc::new("lib", "lib-", input_dir.clone(), mapping);
        let result = nixdoc.execute(&Default::default(), &empty_file);

        match result {
            Ok(()) => panic!("Nixdoc execution should've failed"),
            Err(e) if e.to_string().contains("nixdoc command error") => {
                println!("nixdoc command failed on empty file, which is expected behavior");
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_nixdoc_execute_path_mapping_failure() {
        struct FailingMapper;

        impl PathMapping for FailingMapper {
            type Config = ();

            fn resolve(&self, _config: &Self::Config, _path: &Path) -> Result<PathAction> {
                Err(anyhow!("Mock path mapping failure"))
            }
        }

        let (_temp_dir, input_dir, _output_dir) = setup_test_dirs();

        let test_file = input_dir.join("test.nix");
        fs::write(&test_file, "# Test file\n# Description\n{ lib }: {}").unwrap();

        let failing_mapper = FailingMapper;
        let nixdoc = AutoNixdoc::new("lib", "lib-", input_dir.clone(), failing_mapper);
        let result = nixdoc.execute(&Default::default(), &test_file);

        assert!(result.is_err());
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("path mapping failed"));
        assert!(error_msg.contains("Mock path mapping failure"));
    }

    #[test]
    fn test_nixdoc_command_basic() {
        let nixdoc = Nixdoc::builder()
            .category("test-category")
            .description("test description")
            .file("test-file.nix")
            .build();

        let command = nixdoc.into_command();
        let args: Vec<&std::ffi::OsStr> = command.get_args().collect();
        let program = command.get_program();

        assert_eq!(program, "nixdoc");
        assert_eq!(args.len(), 6);
        assert_eq!(args[0], "--category");
        assert_eq!(args[1], "test-category");
        assert_eq!(args[2], "--description");
        assert_eq!(args[3], "test description");
        assert_eq!(args[4], "--file");
        assert_eq!(args[5], "test-file.nix");
    }

    #[test]
    fn test_nixdoc_command_with_prefix() {
        let nixdoc = Nixdoc::builder()
            .category("lib")
            .description("Library functions")
            .file("lib.nix")
            .prefix("mylib")
            .build();

        let command = nixdoc.into_command();
        let args: Vec<&std::ffi::OsStr> = command.get_args().collect();

        assert_eq!(args.len(), 8);
        assert_eq!(args[6], "--prefix");
        assert_eq!(args[7], "mylib");
    }

    #[test]
    fn test_nixdoc_command_with_anchor_prefix() {
        let nixdoc = Nixdoc::builder()
            .category("utils")
            .description("Utility functions")
            .file("utils.nix")
            .anchor_prefix("util-")
            .build();

        let command = nixdoc.into_command();
        let args: Vec<&std::ffi::OsStr> = command.get_args().collect();

        assert_eq!(args.len(), 8);
        assert_eq!(args[6], "--anchor-prefix");
        assert_eq!(args[7], "util-");
    }

    #[test]
    fn test_nixdoc_command_with_all_options() {
        let nixdoc = Nixdoc::builder()
            .category("full")
            .description("Full test")
            .file("full.nix")
            .prefix("test-prefix")
            .anchor_prefix("test-anchor-")
            .build();

        let command = nixdoc.into_command();
        let args: Vec<&std::ffi::OsStr> = command.get_args().collect();

        assert_eq!(args.len(), 10);
        assert_eq!(args[0], "--category");
        assert_eq!(args[1], "full");
        assert_eq!(args[2], "--description");
        assert_eq!(args[3], "Full test");
        assert_eq!(args[4], "--file");
        assert_eq!(args[5], "full.nix");
        assert_eq!(args[6], "--prefix");
        assert_eq!(args[7], "test-prefix");
        assert_eq!(args[8], "--anchor-prefix");
        assert_eq!(args[9], "test-anchor-");
    }

    #[test]
    fn test_nixdoc_command_empty_strings() {
        let nixdoc = Nixdoc::builder()
            .category("")
            .description("")
            .file("")
            .build();

        let command = nixdoc.into_command();
        let args: Vec<&std::ffi::OsStr> = command.get_args().collect();

        assert_eq!(args.len(), 6);
        assert_eq!(args[1], "");
        assert_eq!(args[3], "");
        assert_eq!(args[5], "");
    }

    #[test]
    fn test_nixdoc_into_trait() {
        let nixdoc = Nixdoc::builder()
            .category("trait-test")
            .description("Testing Into trait")
            .file("trait.nix")
            .build();

        let command: Command = nixdoc.into();
        let args: Vec<&std::ffi::OsStr> = command.get_args().collect();

        assert_eq!(command.get_program(), "nixdoc");
        assert_eq!(args[1], "trait-test");
        assert_eq!(args[3], "Testing Into trait");
        assert_eq!(args[5], "trait.nix");
    }

    #[test]
    fn test_category_extraction_with_path_components() {
        const TEST_NIX_CONTENT: &str = include_str!("../resources/test-lib.nix");

        let (_temp_dir, input_dir, output_dir) = setup_test_dirs();

        let subdir = input_dir.join("utils").join("string");
        fs::create_dir_all(&subdir).unwrap();

        let test_nix_file = subdir.join("helpers.nix");
        fs::write(&test_nix_file, TEST_NIX_CONTENT).unwrap();

        let mapping = AutoMapping::new(&input_dir, &output_dir);
        let nixdoc = AutoNixdoc::new("lib", "lib-", input_dir.clone(), mapping);

        nixdoc
            .execute(&Default::default(), &test_nix_file)
            .expect("Failed to execute");

        let expected_output = output_dir.join("utils").join("string").join("helpers.md");
        assert!(expected_output.exists(), "Output file should be created");

        let content = fs::read_to_string(&expected_output).unwrap();
        assert!(
            content.contains("lib.utils.string.helpers"),
            "Output should contain category with path components: lib.utils.string.helpers, but got: {}",
            content
        );
    }

    #[test]
    fn test_category_extraction_root_file() {
        const TEST_NIX_CONTENT: &str = include_str!("../resources/test-lib.nix");

        let (_temp_dir, input_dir, output_dir) = setup_test_dirs();

        let test_nix_file = input_dir.join("test-lib.nix");
        fs::write(&test_nix_file, TEST_NIX_CONTENT).unwrap();

        let mapping = AutoMapping::new(&input_dir, &output_dir);
        let nixdoc = AutoNixdoc::new("lib", "lib-", input_dir.clone(), mapping);

        nixdoc
            .execute(&Default::default(), &test_nix_file)
            .expect("Failed to execute");

        let expected_output = output_dir.join("test-lib.md");
        assert!(expected_output.exists(), "Output file should be created");

        let content = fs::read_to_string(&expected_output).unwrap();
        assert!(
            content.contains("test-lib"),
            "Output should contain category name derived from filename, but got: {}",
            content
        );
        assert!(
            content.contains("sec-functions-library-test-lib") || content.contains("test-lib"),
            "Output should reference the root file category correctly, but got: {}",
            content
        );
    }
}
