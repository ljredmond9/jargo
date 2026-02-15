mod cli;
mod commands;
mod errors;
mod manifest;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::New { name, lib } => commands::new::exec(&name, lib),
        Command::Init { lib } => commands::init::exec(lib),
        Command::Build => {
            eprintln!("error: `build` is not yet implemented");
            std::process::exit(1);
        }
        Command::Run { .. } => {
            eprintln!("error: `run` is not yet implemented");
            std::process::exit(1);
        }
        Command::Test => {
            eprintln!("error: `test` is not yet implemented");
            std::process::exit(1);
        }
        Command::Check { .. } => {
            eprintln!("error: `check` is not yet implemented");
            std::process::exit(1);
        }
        Command::Clean => {
            eprintln!("error: `clean` is not yet implemented");
            std::process::exit(1);
        }
        Command::Add { .. } => {
            eprintln!("error: `add` is not yet implemented");
            std::process::exit(1);
        }
        Command::Update => {
            eprintln!("error: `update` is not yet implemented");
            std::process::exit(1);
        }
        Command::Tree => {
            eprintln!("error: `tree` is not yet implemented");
            std::process::exit(1);
        }
        Command::Fmt => {
            eprintln!("error: `fmt` is not yet implemented");
            std::process::exit(1);
        }
        Command::Fix => {
            eprintln!("error: `fix` is not yet implemented");
            std::process::exit(1);
        }
        Command::Doc => {
            eprintln!("error: `doc` is not yet implemented");
            std::process::exit(1);
        }
    }
}
