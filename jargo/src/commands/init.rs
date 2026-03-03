use std::path::Path;

use anyhow::Result;

use crate::commands::new::{scaffold, validate_name};
use jargo_core::context::GlobalContext;
use jargo_core::errors::JargoError;

/// Execute `jargo init`.
pub fn exec(gctx: &GlobalContext, is_lib: bool) -> Result<()> {
    if gctx.cwd.join("Jargo.toml").exists() {
        return Err(JargoError::AlreadyInitialized.into());
    }

    let name = dir_name(&gctx.cwd)?;
    validate_name(&name)?;

    scaffold(&gctx.cwd, &name, is_lib)?;

    let kind = if is_lib { "lib" } else { "app" };
    gctx.shell
        .status("Created", &format!("{kind} `{name}` package"));

    Ok(())
}

fn dir_name(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| JargoError::NoDirName.into())
}
