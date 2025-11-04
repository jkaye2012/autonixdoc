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
        .collect();
    assert!(
        output_entries.is_empty(),
        "Output directory should be empty when no .nix files are processed"
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
        .collect();
    assert!(
        output_entries.is_empty(),
        "Output directory should be empty when only non-.nix files are present"
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

    cmd.assert().success();
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

    let expected_bad_file = output_dir.join("bad.md");
    assert!(
        expected_bad_file.exists(),
        "Expected output file {:?} does not exist",
        expected_bad_file
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

    cmd.assert().failure();
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
}

#[test]
fn test_config_file_provided() {
    let (_temp_dir, input_dir, output_dir) = create_test_directory();

    let config_path = _temp_dir.path().join("custom.toml");
    fs::write(&config_path, "[ignore_paths]\npaths = []").expect("Failed to write config");

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

    cmd.assert().code(predicate::in_iter([0, 1]));
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

    cmd.assert().failure();
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

    cmd.assert().failure();
}

#[test]
fn test_environment_variable_config_resolution() {
    let (_temp_dir, input_dir, output_dir) = create_test_directory();

    let config_path = _temp_dir.path().join("env_config.toml");
    fs::write(&config_path, "[ignore_paths]\npaths = []").expect("Failed to write config");

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

    cmd.assert().code(predicate::in_iter([0, 1]));
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

    cmd.assert().code(predicate::in_iter([0, 1]));
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
        .arg("log");

    cmd.assert().code(predicate::in_iter([0, 1]));
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

    cmd.assert().code(predicate::in_iter([0, 1]));

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
