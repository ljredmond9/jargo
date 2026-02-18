use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::manifest::JargoToml;

/// Assemble JAR file from compiled classes and resources.
pub fn assemble_jar(project_root: &Path, manifest: &JargoToml) -> Result<PathBuf> {
    let jar_name = format!("{}.jar", manifest.package.name);
    let jar_path = project_root.join("target").join(&jar_name);

    let file = File::create(&jar_path)
        .with_context(|| format!("failed to create JAR file at {}", jar_path.display()))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    // 1. Write MANIFEST.MF
    write_manifest(&mut zip, manifest, options)?;

    // 2. Add all .class files from target/classes/
    let classes_dir = project_root.join("target/classes");
    if classes_dir.exists() {
        add_directory_to_zip(&mut zip, &classes_dir, &classes_dir, options)?;
    }

    zip.finish()
        .with_context(|| "failed to finish writing JAR file")?;

    Ok(jar_path)
}

fn write_manifest(
    zip: &mut ZipWriter<File>,
    manifest: &JargoToml,
    options: SimpleFileOptions,
) -> Result<()> {
    zip.add_directory("META-INF/", options)
        .with_context(|| "failed to add META-INF directory")?;
    zip.start_file("META-INF/MANIFEST.MF", options)
        .with_context(|| "failed to start MANIFEST.MF file")?;

    let mut content = String::from("Manifest-Version: 1.0\n");

    // For app projects, add Main-Class entry
    if manifest.is_app() {
        let base_package = manifest.get_base_package();
        let main_class = manifest.get_main_class();
        let main_class_fqn = format!("{}.{}", base_package, main_class);
        content.push_str(&format!("Main-Class: {}\n", main_class_fqn));
    }

    zip.write_all(content.as_bytes())
        .with_context(|| "failed to write MANIFEST.MF content")?;
    Ok(())
}

fn add_directory_to_zip(
    zip: &mut ZipWriter<File>,
    source_dir: &Path,
    base_dir: &Path,
    options: SimpleFileOptions,
) -> Result<()> {
    for entry in fs::read_dir(source_dir)
        .with_context(|| format!("failed to read directory {}", source_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let relative_path = path
            .strip_prefix(base_dir)
            .with_context(|| "failed to compute relative path")?;

        if path.is_dir() {
            // Recursively add subdirectories
            add_directory_to_zip(zip, &path, base_dir, options)?;
        } else {
            // Add file to ZIP
            let zip_path = relative_path.to_string_lossy().replace('\\', "/");
            zip.start_file(&zip_path, options)
                .with_context(|| format!("failed to start file {} in JAR", zip_path))?;
            let file_contents = fs::read(&path)
                .with_context(|| format!("failed to read file {}", path.display()))?;
            zip.write_all(&file_contents)
                .with_context(|| format!("failed to write file {} to JAR", zip_path))?;
        }
    }
    Ok(())
}
