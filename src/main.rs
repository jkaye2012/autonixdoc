use anyhow::Result;
use autonixdoc::cli::Driver;
use clap::Parser;

fn main() -> Result<()> {
    Driver::parse().run()
}
