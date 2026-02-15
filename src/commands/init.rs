use std::env;
use std::path::Path;

use anyhow::{Context, Result};

use crate::commands::new::{scaffold, validate_name};
use crate::errors::JargoError;

/// Execute `jargo init`.
pub fn exec(is_lib: bool) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;

    if cwd.join("Jargo.toml").exists() {
        return Err(JargoError::AlreadyInitialized.into());
    }

    let name = dir_name(&cwd)?;
    validate_name(&name)?;

    scaffold(&cwd, &name, is_lib)?;

    let kind = if is_lib { "lib" } else { "app" };
    println!("    Created {kind} `{name}` package");

    Ok(())
}

fn dir_name(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| JargoError::NoDirName.into())
}
