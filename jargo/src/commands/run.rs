use anyhow::Result;
use std::env;
use std::process::Command;

use jargo_core::compiler;
use jargo_core::errors::JargoError;
use jargo_core::manifest::JargoToml;
use jargo_core::resolver;

pub fn exec(args: Vec<String>) -> Result<()> {
    let cwd = env::current_dir()?;
    let manifest_path = cwd.join("Jargo.toml");

    if !manifest_path.exists() {
        return Err(JargoError::ManifestNotFound.into());
    }

    let manifest = JargoToml::from_file(&manifest_path)
        .map_err(|e| JargoError::ManifestParse(e.to_string()))?;

    // run is app-only
    if !manifest.is_app() {
        return Err(JargoError::NotAnApp.into());
    }

    // Resolve dependencies (uses lock file if present, else resolves + writes lock)
    let resolved = resolver::resolve(&cwd, &manifest)?;

    // Compile
    println!(
        "   Compiling {} v{} (java {})",
        manifest.package.name, manifest.package.version, manifest.package.java
    );

    let compile_output = compiler::compile(&cwd, &manifest, &resolved.compile_jars)?;

    if !compile_output.success {
        for error in compile_output.errors {
            eprintln!("{}", error);
        }
        return Err(JargoError::CompilationFailed.into());
    }

    // Assemble the runtime classpath: compiled classes + dependency JARs.
    let classes_dir = cwd.join("target/classes");

    #[cfg(windows)]
    let sep = ";";
    #[cfg(not(windows))]
    let sep = ":";

    let mut cp_parts = vec![classes_dir.to_string_lossy().into_owned()];
    for jar in &resolved.runtime_jars {
        cp_parts.push(jar.to_string_lossy().into_owned());
    }
    let classpath = cp_parts.join(sep);

    // Build the fully-qualified main class name
    let base_package = manifest.get_base_package();
    let main_class = manifest.get_main_class();
    let fq_main_class = format!("{}.{}", base_package, main_class);

    // Invoke java
    println!("     Running {}", manifest.package.name);

    let jvm_args = manifest.get_jvm_args();

    let status = Command::new("java")
        .arg("-cp")
        .arg(&classpath)
        .args(jvm_args)
        .arg(&fq_main_class)
        .args(&args)
        .current_dir(&cwd)
        .status()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                JargoError::JavaNotFound
            } else {
                e.into()
            }
        })?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}
