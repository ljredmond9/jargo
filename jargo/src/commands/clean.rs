use anyhow::Result;
use std::fs;

use jargo_core::context::GlobalContext;

pub fn exec(gctx: &GlobalContext) -> Result<()> {
    let target = gctx.cwd.join("target");

    if target.exists() {
        fs::remove_dir_all(&target)?;
        println!("     Removed target directory");
    } else {
        println!("     Nothing to clean");
    }

    Ok(())
}
