mod cli;
mod commands;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let gctx = jargo_core::context::GlobalContext::new(cli.verbose)?;

    match cli.command {
        Command::New { name, lib } => commands::new::exec(&gctx, &name, lib),
        Command::Init { lib } => commands::init::exec(&gctx, lib),
        Command::Build => commands::build::exec(&gctx),
        Command::Run { args } => commands::run::exec(&gctx, args),
        Command::Test => {
            eprintln!("error: `test` is not yet implemented");
            std::process::exit(1);
        }
        Command::Check { .. } => {
            eprintln!("error: `check` is not yet implemented");
            std::process::exit(1);
        }
        Command::Clean => commands::clean::exec(&gctx),
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
