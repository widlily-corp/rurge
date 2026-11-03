//! Developer CLI tool for Rurge native mods and client launcher.

use clap::Parser;
use cargo_rurge::Cli;

fn main() {
    let cli = Cli::parse();
    if let Err(err) = cli.execute() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
