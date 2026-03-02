use anyhow::Result;
use std::env;

use jargo_core::compiler;
use jargo_core::errors::JargoError;
use jargo_core::jar;
use jargo_core::manifest::JargoToml;
use jargo_core::resolver;

pub fn exec() -> Result<()> {
    let cwd = env::current_dir()?;
    let manifest_path = cwd.join("Jargo.toml");

    if !manifest_path.exists() {
        return Err(JargoError::ManifestNotFound.into());
    }

    // Load manifest
    let manifest = JargoToml::from_file(&manifest_path)
        .map_err(|e| JargoError::ManifestParse(e.to_string()))?;

    // Resolve dependencies (uses lock file if present, else resolves + writes lock)
    let resolved = resolver::resolve(&cwd, &manifest)?;

    // Print Cargo-style compilation status
    println!(
        "   Compiling {} v{} (java {})",
        manifest.package.name, manifest.package.version, manifest.package.java
    );

    // Compile with dependency classpath
    let compile_output = compiler::compile(&cwd, &manifest, &resolved.compile_jars)?;

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
