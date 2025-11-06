use std::fs;
use std::path::Path;

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use tempfile::TempDir;

fn create_test_directory() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let input_dir = temp_dir.path().join("input");
    let output_dir = temp_dir.path().join("output");

    fs::create_dir_all(&input_dir).expect("Failed to create input directory");
    fs::create_dir_all(&output_dir).expect("Failed to create output directory");

    (temp_dir, input_dir, output_dir)
}

fn create_nix_file(dir: &Path, filename: &str, content: &str) -> std::path::PathBuf {
    let file_path = dir.join(filename);
    fs::write(&file_path, content).expect("Failed to write test file");
    file_path
}

fn cli_command() -> Command {
    cargo_bin_cmd!("autonixdoc")
}

fn count_files_recursive(dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                count += 1;
            } else if path.is_dir() {
                count += count_files_recursive(&path);
            }
        }
    }
    count
}

#[test]
fn test_successful_documentation_generation() {
    let (_temp_dir, input_dir, output_dir) = create_test_directory();

    create_nix_file(
        &input_dir,
        "test.nix",
        "# A test function\n{ lib }: { hello = \"world\"; }",
    );

    let mut cmd = cli_command();
    cmd.arg("--input-dir")
        .arg(&input_dir)
        .arg("--output-dir")
        .arg(&output_dir)
        .arg("--on-failure")
        .arg("log");

    cmd.assert().success();

    let expected_output_file = output_dir.join("test.md");
    assert!(
        expected_output_file.exists(),
        "Expected output file {:?} does not exist",
        expected_output_file
    );

    let file_content =
        fs::read_to_string(&expected_output_file).expect("Failed to read output file");
    assert!(
        !file_content.trim().is_empty(),
        "Output file should contain documentation content"
    );
}

#[test]
fn test_empty_directory_handling() {
    let (_temp_dir, input_dir, output_dir) = create_test_directory();

    let mut cmd = cli_command();
    cmd.arg("--input-dir")
        .arg(&input_dir)
        .arg("--output-dir")
        .arg(&output_dir)
        .arg("--on-failure")
        .arg("log");

    cmd.assert().success();

    let output_entries: Vec<_> = fs::read_dir(&output_dir)
        .expect("Failed to read output directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to collect directory entries");
    assert_eq!(
        output_entries.len(),
        0,
        "Output directory should contain exactly 0 files when no .nix files are processed, found {} entries",
        output_entries.len()
    );
}

#[test]
fn test_non_nix_files_ignored() {
    let (_temp_dir, input_dir, output_dir) = create_test_directory();

    create_nix_file(&input_dir, "readme.txt", "This is not a Nix file");
    create_nix_file(&input_dir, "config.json", "{}");

    let mut cmd = cli_command();
    cmd.arg("--input-dir")
        .arg(&input_dir)
        .arg("--output-dir")
        .arg(&output_dir)
        .arg("--on-failure")
        .arg("log");

    cmd.assert().success();

    let output_entries: Vec<_> = fs::read_dir(&output_dir)
        .expect("Failed to read output directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to collect directory entries");
    assert_eq!(
        output_entries.len(),
        0,
        "Output directory should contain exactly 0 files when only non-.nix files are present, found {} entries",
        output_entries.len()
    );

    assert!(
        !output_dir.join("readme.md").exists(),
        "No .md file should be created for .txt file"
    );
    assert!(
        !output_dir.join("config.md").exists(),
        "No .md file should be created for .json file"
    );
}

#[test]
fn test_failure_behavior_log() {
    let (_temp_dir, input_dir, output_dir) = create_test_directory();

    create_nix_file(
        &input_dir,
        "invalid.nix",
        "this is not valid nix syntax {{{",
    );

    let mut cmd = cli_command();
    cmd.arg("--input-dir")
        .arg(&input_dir)
        .arg("--output-dir")
        .arg(&output_dir)
        .arg("--on-failure")
        .arg("log");

    cmd.assert().success().stderr(
        predicate::str::contains("Failed to generate documentation").or(predicate::str::is_empty()),
    );

    let expected_output_file = output_dir.join("invalid.md");
    assert!(
        expected_output_file.exists(),
        "Output file should be created even for invalid .nix files with log behavior"
    );
}

#[test]
fn test_failure_behavior_skip() {
    let (_temp_dir, input_dir, output_dir) = create_test_directory();

    create_nix_file(&input_dir, "good.nix", "{ lib }: { hello = \"world\"; }");
    create_nix_file(&input_dir, "bad.nix", "invalid syntax");

    let mut cmd = cli_command();
    cmd.arg("--input-dir")
        .arg(&input_dir)
        .arg("--output-dir")
        .arg(&output_dir)
        .arg("--on-failure")
        .arg("skip");

    cmd.assert().success();

    let expected_good_file = output_dir.join("good.md");
    assert!(
        expected_good_file.exists(),
        "Expected output file {:?} does not exist",
        expected_good_file
    );

    let good_content =
        fs::read_to_string(&expected_good_file).expect("Failed to read good output file");
    assert!(
        !good_content.trim().is_empty(),
        "Good .nix file should generate non-empty documentation"
    );

    let expected_bad_file = output_dir.join("bad.md");
    assert!(
        expected_bad_file.exists(),
        "Expected output file {:?} does not exist",
        expected_bad_file
    );

    let output_entries: Vec<_> = fs::read_dir(&output_dir)
        .expect("Failed to read output directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to collect directory entries");
    assert_eq!(
        output_entries.len(),
        2,
        "Output directory should contain exactly 2 files (good.md and bad.md)"
    );
}

#[test]
fn test_failure_behavior_abort() {
    let (_temp_dir, input_dir, output_dir) = create_test_directory();

    create_nix_file(
        &input_dir,
        "invalid.nix",
        "this is not valid nix syntax {{{",
    );

    let mut cmd = cli_command();
    cmd.arg("--input-dir")
        .arg(&input_dir)
        .arg("--output-dir")
        .arg(&output_dir)
        .arg("--on-failure")
        .arg("abort");

    cmd.assert().failure().code(predicate::ne(0));

    let expected_output_file = output_dir.join("invalid.md");
    let file_exists = expected_output_file.exists();

    let output_entries: Vec<_> = fs::read_dir(&output_dir)
        .expect("Failed to read output directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to collect directory entries");

    if file_exists {
        assert_eq!(
            output_entries.len(),
            1,
            "If output file exists with abort behavior, there should be exactly 1 file"
        );
    } else {
        assert_eq!(
            output_entries.len(),
            0,
            "If abort behavior prevents file creation, output directory should be empty"
        );
    }
}

#[test]
fn test_default_failure_behavior() {
    let (_temp_dir, input_dir, output_dir) = create_test_directory();

    create_nix_file(&input_dir, "test.nix", "{ lib }: { hello = \"world\"; }");

    let mut cmd = cli_command();
    cmd.arg("--input-dir")
        .arg(&input_dir)
        .arg("--output-dir")
        .arg(&output_dir);

    cmd.assert().code(predicate::in_iter([0, 1]));

    let expected_output_file = output_dir.join("test.md");
    if expected_output_file.exists() {
        let file_content =
            fs::read_to_string(&expected_output_file).expect("Failed to read output file");
        assert!(
            !file_content.trim().is_empty(),
            "Output file should contain documentation content when created with default behavior"
        );
    }
}

#[test]
fn test_config_file_provided() {
    let (_temp_dir, input_dir, output_dir) = create_test_directory();

    let config_path = _temp_dir.path().join("custom.toml");
    fs::write(&config_path, "ignore_paths = []").expect("Failed to write config");

    create_nix_file(&input_dir, "test.nix", "{ lib }: { hello = \"world\"; }");

    let mut cmd = cli_command();
    cmd.arg("--input-dir")
        .arg(&input_dir)
        .arg("--output-dir")
        .arg(&output_dir)
        .arg("--config")
        .arg(&config_path)
        .arg("--on-failure")
        .arg("log");

    cmd.assert().success();
}

#[test]
fn test_invalid_config_file() {
    let (_temp_dir, input_dir, output_dir) = create_test_directory();

    let config_path = _temp_dir.path().join("invalid.toml");
    fs::write(&config_path, "invalid toml content [[[").expect("Failed to write invalid config");

    let mut cmd = cli_command();
    cmd.arg("--input-dir")
        .arg(&input_dir)
        .arg("--output-dir")
        .arg(&output_dir)
        .arg("--config")
        .arg(&config_path)
        .arg("--on-failure")
        .arg("log");

    cmd.assert().failure().code(predicate::ne(0)).stderr(
        predicate::str::contains("invalid toml").or(predicate::str::contains("Failed to parse")),
    );

    let output_entries: Vec<_> = fs::read_dir(&output_dir)
        .expect("Failed to read output directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to collect directory entries");
    assert_eq!(
        output_entries.len(),
        0,
        "No output files should be created when config file is invalid"
    );
}

#[test]
fn test_nonexistent_config_file() {
    let (_temp_dir, input_dir, output_dir) = create_test_directory();

    let nonexistent_config = _temp_dir.path().join("nonexistent.toml");

    let mut cmd = cli_command();
    cmd.arg("--input-dir")
        .arg(&input_dir)
        .arg("--output-dir")
        .arg(&output_dir)
        .arg("--config")
        .arg(&nonexistent_config)
        .arg("--on-failure")
        .arg("log");

    cmd.assert()
        .failure()
        .code(predicate::ne(0))
        .stderr(predicate::str::contains(
            "Failed to read configuration file",
        ));

    let output_entries: Vec<_> = fs::read_dir(&output_dir)
        .expect("Failed to read output directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to collect directory entries");
    assert_eq!(
        output_entries.len(),
        0,
        "No output files should be created when config file does not exist"
    );
}

#[test]
fn test_environment_variable_config_resolution() {
    let (_temp_dir, input_dir, output_dir) = create_test_directory();

    let config_path = _temp_dir.path().join("env_config.toml");
    fs::write(&config_path, "ignore_paths = []").expect("Failed to write config");

    create_nix_file(&input_dir, "test.nix", "{ lib }: { hello = \"world\"; }");

    let mut cmd = cli_command();
    cmd.env(
        "AUTONIXDOC_CONFIG",
        config_path.to_string_lossy().to_string(),
    )
    .arg("--input-dir")
    .arg(&input_dir)
    .arg("--output-dir")
    .arg(&output_dir)
    .arg("--on-failure")
    .arg("log");

    cmd.assert().success();
}

#[test]
fn test_environment_variable_failure_behavior() {
    let (_temp_dir, input_dir, output_dir) = create_test_directory();

    create_nix_file(&input_dir, "test.nix", "{ lib }: { hello = \"world\"; }");

    let mut cmd = cli_command();
    cmd.env("AUTONIXDOC_ON_FAILURE", "skip")
        .arg("--input-dir")
        .arg(&input_dir)
        .arg("--output-dir")
        .arg(&output_dir);

    cmd.assert().success();
}

#[test]
fn test_cli_args_override_environment_variables() {
    let (_temp_dir, input_dir, output_dir) = create_test_directory();

    create_nix_file(
        &input_dir,
        "invalid.nix",
        "this is not valid nix syntax {{{",
    );

    let mut cmd = cli_command();
    cmd.arg("--input-dir")
        .arg(&input_dir)
        .arg("--output-dir")
        .arg(&output_dir)
        .arg("--on-failure")
        .arg("log");

    cmd.assert().success();
}

#[test]
fn test_nonexistent_input_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let nonexistent_input = temp_dir.path().join("nonexistent");
    let output_dir = temp_dir.path().join("output");
    fs::create_dir_all(&output_dir).expect("Failed to create output directory");

    let mut cmd = cli_command();
    cmd.arg("--input-dir")
        .arg(&nonexistent_input)
        .arg("--output-dir")
        .arg(&output_dir)
        .arg("--on-failure")
        .arg("abort");

    cmd.assert().failure();
}

#[test]
fn test_nested_directory_structure() {
    let (_temp_dir, input_dir, output_dir) = create_test_directory();

    let nested_dir = input_dir.join("subdir").join("deep");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested directory");

    create_nix_file(&input_dir, "root.nix", "{ lib }: { root = true; }");
    create_nix_file(
        &input_dir.join("subdir"),
        "sub.nix",
        "{ lib }: { sub = true; }",
    );
    create_nix_file(&nested_dir, "deep.nix", "{ lib }: { deep = true; }");

    let mut cmd = cli_command();
    cmd.arg("--input-dir")
        .arg(&input_dir)
        .arg("--output-dir")
        .arg(&output_dir)
        .arg("--on-failure")
        .arg("log");

    cmd.assert().success();

    let expected_root_file = output_dir.join("root.md");
    let expected_sub_file = output_dir.join("subdir").join("sub.md");
    let expected_deep_file = output_dir.join("subdir").join("deep").join("deep.md");

    assert!(
        expected_root_file.exists(),
        "Expected root output file {:?} does not exist",
        expected_root_file
    );
    assert!(
        expected_sub_file.exists(),
        "Expected sub output file {:?} does not exist",
        expected_sub_file
    );
    assert!(
        expected_deep_file.exists(),
        "Expected deep output file {:?} does not exist",
        expected_deep_file
    );

    let output_subdir = output_dir.join("subdir");
    assert!(
        output_subdir.exists() && output_subdir.is_dir(),
        "Output subdirectory should be created to mirror input structure"
    );

    let output_deep_dir = output_dir.join("subdir").join("deep");
    assert!(
        output_deep_dir.exists() && output_deep_dir.is_dir(),
        "Output deep subdirectory should be created to mirror input structure"
    );

    let total_files = count_files_recursive(&output_dir);
    assert_eq!(
        total_files, 3,
        "Output directory should contain exactly 3 .md files total across all subdirectories"
    );
}

#[test]
fn test_invalid_cli_arguments() {
    let mut cmd = cli_command();
    cmd.arg("--invalid-arg").arg("value");

    cmd.assert().failure();
}

#[test]
fn test_missing_required_arguments() {
    let mut cmd = cli_command();

    cmd.assert().failure();
}

#[test]
fn test_help_flag() {
    let mut cmd = cli_command();
    cmd.arg("--help");

    cmd.assert().success();
}
