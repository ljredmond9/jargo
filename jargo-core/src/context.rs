use anyhow::{Context, Result};
use std::path::PathBuf;

pub struct GlobalContext {
    pub verbose: bool,
    pub jargo_home: PathBuf, // ~/.jargo/
    pub cwd: PathBuf,
}

impl GlobalContext {
    pub fn new(verbose: bool) -> Result<Self> {
        let cwd = std::env::current_dir().context("could not determine current directory")?;
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .context("could not determine home directory")?;
        let jargo_home = PathBuf::from(home).join(".jargo");
        Ok(Self {
            verbose,
            jargo_home,
            cwd,
        })
    }
}
