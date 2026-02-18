use anyhow::Result;
use std::env;
use std::fs;

pub fn exec() -> Result<()> {
    let cwd = env::current_dir()?;
    let target = cwd.join("target");

    if target.exists() {
        fs::remove_dir_all(&target)?;
        println!("     Removed target directory");
    } else {
        println!("     Nothing to clean");
    }

    Ok(())
}
