use std::{io::Write, process::Command};

fn main() {
    let output = Command::new("nixdoc").arg("--help").output().unwrap();
    std::io::stdout().write_all(&output.stdout).unwrap();
}
