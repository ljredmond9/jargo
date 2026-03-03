use anyhow::Result;
use std::fs;

use jargo_core::context::GlobalContext;

pub fn exec(gctx: &GlobalContext) -> Result<()> {
    let target = gctx.cwd.join("target");

    if target.exists() {
        fs::remove_dir_all(&target)?;
        gctx.shell.status("Removed", "target directory");
    } else {
        gctx.shell.status("Nothing", "to clean");
    }

    Ok(())
}
