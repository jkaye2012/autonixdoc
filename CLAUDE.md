# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this
repository.

## Project Overview

This is a Rust binary project called `autonixdoc` that automatically generates nixdoc documentation
for a source tree. The project is structured as a binary crate with a library component.

## Development Environment

This project uses Nix flakes for development environment management. The flake provides:

- Rust toolchain via fenix
- Development tools including cargo-show-asm, perf, lldb, and nixdoc
- Crane for Rust builds
- Custom devenv integration

## Common Commands

**Enter development shell:**

```bash
nix develop
```

**Build the project:**

```bash
cargo build
```

**Run the project:**

```bash
cargo run
```

**Check/lint the code:**

```bash
cargo check
cargo clippy
```

**Run tests:**

```bash
cargo test
```

**Build via Nix:**

```bash
nix build
```

## Architecture

- **src/main.rs**: Entry point that currently demonstrates nixdoc usage
- **src/lib.rs**: Library module declarations
- **src/nixdoc.rs**: Core nixdoc functionality module
- **flake.nix**: Nix flake defining the build and development environment using crane, fenix, and
  custom devenv

The project follows the standard Rust binary + library pattern where main.rs provides the CLI
interface and lib.rs exposes reusable functionality.

## Build System

The project uses both Cargo and Nix for building:

- Cargo for standard Rust development workflow
- Nix flake with crane for reproducible builds and packaging
- The flake is configured with Rust edition 2024 and includes performance debugging tools

## Testing

When adding tests, always add them in a `tests` module within the file that contains the
implementations to be tested. Tests should be written both for happy-path and failure cases whenever
possible. Mocks should never be used unless explicitly requested. Tests should always be named after
the _specific_ functionality that they're testing.

## Making Changes

Whenever _any_ changes are made to the project, always verify that tests are passing by running
`cargo test` as the last step of your todo list.
