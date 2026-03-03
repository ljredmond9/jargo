use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::shell::{Shell, Verbosity};

pub struct GlobalContext {
    pub jargo_home: PathBuf, // ~/.jargo/
    pub cwd: PathBuf,
    pub shell: Shell,
}

impl GlobalContext {
    pub fn new(verbose: bool) -> Result<Self> {
        let cwd = std::env::current_dir().context("could not determine current directory")?;
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .context("could not determine home directory")?;
        let jargo_home = PathBuf::from(home).join(".jargo");
        let verbosity = if verbose {
            Verbosity::Verbose
        } else {
            Verbosity::Normal
        };
        Ok(Self {
            shell: Shell::new(verbosity),
            jargo_home,
            cwd,
        })
    }
}
