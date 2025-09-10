use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{anyhow, Context, Result};
use typed_builder::TypedBuilder;

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

fn mirror_path(source_path: &Path, source_base: &Path, dest_base: &Path) -> Result<PathBuf> {
    let source_dir = source_path
        .parent()
        .with_context(|| "source path had no parent")?;
    let relative_path = source_dir
        .strip_prefix(source_base)
        .expect("Source directory isn't a prefix of source path? Please report this, it's a bug");

    let source_stem = source_path
        .file_stem()
        .with_context(|| "source path had no file name")?;

    Ok(dest_base
        .to_path_buf()
        .join(relative_path)
        .join(source_stem)
        .with_extension("md"))
}

/// Automated nixdoc documentation generator.
///
/// This struct provides high-level automation for generating nixdoc documentation
/// from source files. It handles the complete workflow from reading source files
/// to generating markdown documentation.
pub struct AutoNixdoc<'a> {
    /// Prefix for generated identifiers
    prefix: &'a str,
    /// Prefix for anchor links in documentation
    anchor_prefix: &'a str,
    /// Root directory of the source files
    input_root: &'a Path,
    /// Root directory for documentation output
    output_root: &'a Path,
}

impl<'a> AutoNixdoc<'a> {
    /// Creates a new AutoNixdoc instance.
    ///
    /// # Arguments
    ///
    /// * `prefix` - Prefix for generated identifiers in the documentation
    /// * `anchor_prefix` - Prefix for anchor links in the generated documentation
    /// * `input_root` - Root directory containing the source files to document
    /// * `output_root` - Root directory where documentation will be written
    pub fn new(
        prefix: &'a str,
        anchor_prefix: &'a str,
        input_root: &'a Path,
        output_root: &'a Path,
    ) -> Self {
        AutoNixdoc {
            prefix,
            anchor_prefix,
            input_root,
            output_root,
        }
    }

    /// Generates documentation for a single source file.
    ///
    /// This function processes a source file and generates corresponding markdown
    /// documentation using nixdoc.
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
    /// - The source file cannot be read
    /// - The output directory cannot be created
    /// - The nixdoc command fails
    pub fn execute<P: AsRef<Path>>(&self, path_ref: P) -> Result<()> {
        let path = path_ref.as_ref();
        let path_str = path
            .to_str()
            .with_context(|| "source path was not valid unicode")?;
        let category = path
            .file_stem()
            .and_then(std::ffi::OsStr::to_str) // We know this will succeed because of the conversion above
            .with_context(|| "source path had no file name")?;

        let dest_path = mirror_path(&path, self.input_root, self.output_root)?;
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
        let desc = reader
            .lines()
            .nth(1)
            .transpose()
            .with_context(|| format!("Failed to read input file: {}", path_str))?
            .unwrap_or_default();

        let nixdoc = Nixdoc::builder()
            .file(path_str)
            .category(category)
            .description(&desc)
            .prefix(self.prefix)
            .anchor_prefix(self.anchor_prefix)
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
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_mirror_path_absolute_basic() {
        let source_path = PathBuf::from("/src/lib/module.nix");
        let source_base = PathBuf::from("/src");
        let dest_base = PathBuf::from("/docs");

        let result = mirror_path(&source_path, &source_base, &dest_base).unwrap();
        let expected = PathBuf::from("/docs/lib/module.md");

        assert_eq!(result, expected);
    }

    #[test]
    fn test_mirror_path_absolute_nested() {
        let source_path = PathBuf::from("/project/src/deep/nested/file.nix");
        let source_base = PathBuf::from("/project/src");
        let dest_base = PathBuf::from("/output");

        let result = mirror_path(&source_path, &source_base, &dest_base).unwrap();
        let expected = PathBuf::from("/output/deep/nested/file.md");

        assert_eq!(result, expected);
    }

    #[test]
    fn test_mirror_path_absolute_root_level() {
        let source_path = PathBuf::from("/src/default.nix");
        let source_base = PathBuf::from("/src");
        let dest_base = PathBuf::from("/docs");

        let result = mirror_path(&source_path, &source_base, &dest_base).unwrap();
        let expected = PathBuf::from("/docs/default.md");

        assert_eq!(result, expected);
    }

    #[test]
    fn test_mirror_path_relative_basic() {
        let source_path = PathBuf::from("src/lib/module.nix");
        let source_base = PathBuf::from("src");
        let dest_base = PathBuf::from("docs");

        let result = mirror_path(&source_path, &source_base, &dest_base).unwrap();
        let expected = PathBuf::from("docs/lib/module.md");

        assert_eq!(result, expected);
    }

    #[test]
    fn test_mirror_path_relative_nested() {
        let source_path = PathBuf::from("project/src/deep/nested/file.nix");
        let source_base = PathBuf::from("project/src");
        let dest_base = PathBuf::from("output");

        let result = mirror_path(&source_path, &source_base, &dest_base).unwrap();
        let expected = PathBuf::from("output/deep/nested/file.md");

        assert_eq!(result, expected);
    }

    #[test]
    fn test_mirror_path_relative_root_level() {
        let source_path = PathBuf::from("src/default.nix");
        let source_base = PathBuf::from("src");
        let dest_base = PathBuf::from("docs");

        let result = mirror_path(&source_path, &source_base, &dest_base).unwrap();
        let expected = PathBuf::from("docs/default.md");

        assert_eq!(result, expected);
    }

    #[test]
    fn test_mirror_path_mixed_absolute_relative() {
        let source_path = PathBuf::from("/absolute/src/file.nix");
        let source_base = PathBuf::from("/absolute/src");
        let dest_base = PathBuf::from("relative/docs");

        let result = mirror_path(&source_path, &source_base, &dest_base).unwrap();
        let expected = PathBuf::from("relative/docs/file.md");

        assert_eq!(result, expected);
    }

    #[test]
    fn test_mirror_path_no_parent_error() {
        let source_path = PathBuf::from("/");
        let source_base = PathBuf::from("/src");
        let dest_base = PathBuf::from("/docs");

        let result = mirror_path(&source_path, &source_base, &dest_base);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "source path had no parent");
    }

    #[test]
    fn test_mirror_path_no_file_stem_error() {
        let source_path = PathBuf::from("/src/..");
        let source_base = PathBuf::from("/src");
        let dest_base = PathBuf::from("/docs");

        let result = mirror_path(&source_path, &source_base, &dest_base);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "source path had no file name"
        );
    }

    #[test]
    fn test_can_use_current_directory() {
        let source_path = PathBuf::from("./example.nix");
        let source_base = PathBuf::from(".");
        let dest_base = PathBuf::from("docs/");

        let result = mirror_path(&source_path, &source_base, &dest_base).unwrap();
        assert_eq!(result, PathBuf::from("docs/example.md"));
    }

    #[test]
    #[should_panic(
        expected = "Source directory isn't a prefix of source path? Please report this, it's a bug"
    )]
    fn test_mirror_path_invalid_prefix_panic() {
        let source_path = PathBuf::from("/other/lib/module.nix");
        let source_base = PathBuf::from("/src");
        let dest_base = PathBuf::from("/docs");

        let _ = mirror_path(&source_path, &source_base, &dest_base);
    }

    #[test]
    #[should_panic(
        expected = "Source directory isn't a prefix of source path? Please report this, it's a bug"
    )]
    fn test_mirror_path_relative_invalid_prefix_panic() {
        let source_path = PathBuf::from("other/lib/module.nix");
        let source_base = PathBuf::from("src");
        let dest_base = PathBuf::from("docs");

        let _ = mirror_path(&source_path, &source_base, &dest_base);
    }

    #[test]
    fn test_nixdoc_execute_success() {
        use std::fs;

        const TEST_NIX_CONTENT: &str = include_str!("../resources/test-lib.nix");

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let input_dir = temp_path.join("input");
        let output_dir = temp_path.join("output");
        fs::create_dir_all(&input_dir).unwrap();

        let test_nix_file = input_dir.join("test-lib.nix");
        fs::write(&test_nix_file, TEST_NIX_CONTENT).unwrap();

        let nixdoc = AutoNixdoc::new("lib", "lib-", &input_dir, &output_dir);

        let result = nixdoc.execute(&test_nix_file);

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
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }

    #[test]
    fn test_nixdoc_execute_nonexistent_file() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let input_dir = temp_path.join("input");
        let output_dir = temp_path.join("output");
        fs::create_dir_all(&input_dir).unwrap();

        let nonexistent_file = input_dir.join("nonexistent.nix");

        let nixdoc = AutoNixdoc::new("lib", "lib-", &input_dir, &output_dir);
        let result = nixdoc.execute(&nonexistent_file);

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("No such file"));
    }

    #[test]
    fn test_nixdoc_execute_invalid_unicode_path() {
        use std::ffi::OsStr;
        use std::fs;
        use std::os::unix::ffi::OsStrExt;

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let input_dir = temp_path.join("input");
        let output_dir = temp_path.join("output");
        fs::create_dir_all(&input_dir).unwrap();

        // Create a path with invalid UTF-8
        let invalid_utf8 = OsStr::from_bytes(&[0x66, 0x6f, 0x6f, 0x80, 0x2e, 0x6e, 0x69, 0x78]); // "foo<invalid>.nix"
        let invalid_file = input_dir.join(invalid_utf8);

        let nixdoc = AutoNixdoc::new("lib", "lib-", &input_dir, &output_dir);
        let result = nixdoc.execute(&invalid_file);

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("source path was not valid unicode"));
    }

    #[test]
    fn test_nixdoc_execute_read_only_output_directory() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

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

        let nixdoc = AutoNixdoc::new("lib", "lib-", &input_dir, &output_dir);
        let result = nixdoc.execute(&test_nix_file);

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
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let input_dir = temp_path.join("input");
        let output_dir = temp_path.join("output");
        fs::create_dir_all(&input_dir).unwrap();

        let empty_file = input_dir.join("empty.nix");
        fs::write(&empty_file, "").unwrap();

        let nixdoc = AutoNixdoc::new("lib", "lib-", &input_dir, &output_dir);
        let result = nixdoc.execute(&empty_file);

        match result {
            Ok(()) => panic!("Nixdoc execution should've failed"),
            Err(e) if e.to_string().contains("nixdoc command error") => {
                println!("nixdoc command failed on empty file, which is expected behavior");
            }
            Err(e) => panic!("Unexpected error: {}", e),
        }
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
}
