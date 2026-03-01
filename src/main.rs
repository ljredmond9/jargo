mod cache;
mod cli;
mod commands;
mod compiler;
mod errors;
mod jar;
mod lockfile;
mod manifest;
mod staging;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::New { name, lib } => commands::new::exec(&name, lib),
        Command::Init { lib } => commands::init::exec(lib),
        Command::Build => commands::build::exec(),
        Command::Run { args } => commands::run::exec(args),
        Command::Test => {
            eprintln!("error: `test` is not yet implemented");
            std::process::exit(1);
        }
        Command::Check { .. } => {
            eprintln!("error: `check` is not yet implemented");
            std::process::exit(1);
        }
        Command::Clean => commands::clean::exec(),
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
