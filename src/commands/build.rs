use anyhow::Result;
use std::env;

use crate::compiler;
use crate::errors::JargoError;
use crate::jar;
use crate::manifest::JargoToml;

pub fn exec() -> Result<()> {
    let cwd = env::current_dir()?;
    let manifest_path = cwd.join("Jargo.toml");

    if !manifest_path.exists() {
        return Err(JargoError::ManifestNotFound.into());
    }

    // Load manifest
    let manifest = JargoToml::from_file(&manifest_path)
        .map_err(|e| JargoError::ManifestParse(e.to_string()))?;

    // Print Cargo-style compilation status
    println!(
        "   Compiling {} v{} (java {})",
        manifest.package.name, manifest.package.version, manifest.package.java
    );

    // Compile
    let compile_output = compiler::compile(&cwd, &manifest)?;

    if !compile_output.success {
        for error in compile_output.errors {
            eprintln!("{}", error);
        }
        return Err(JargoError::CompilationFailed.into());
    }

    // Assemble JAR
    let jar_path = jar::assemble_jar(&cwd, &manifest)?;

    println!(
        "    Finished JAR at {}",
        jar_path.strip_prefix(&cwd).unwrap_or(&jar_path).display()
    );

    Ok(())
}
