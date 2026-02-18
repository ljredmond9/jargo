use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::errors::JargoError;
use crate::manifest::JargoToml;
use crate::staging;

pub struct CompileOutput {
    pub success: bool,
    pub errors: Vec<String>,
}

/// Compile the project at the given root directory.
pub fn compile(project_root: &Path, manifest: &JargoToml) -> Result<CompileOutput> {
    let base_package = manifest.get_base_package();

    // 1. Create staging symlink
    let src_root = staging::create_staging(project_root, &base_package)?;

    // 2. Ensure target/classes exists
    let classes_dir = project_root.join("target/classes");
    fs::create_dir_all(&classes_dir)
        .with_context(|| format!("failed to create {}", classes_dir.display()))?;

    // 3. Find all source files
    let src_dir = project_root.join("src");
    let source_files = find_java_files(&src_dir)?;

    if source_files.is_empty() {
        return Err(anyhow::anyhow!("no source files found in src/"));
    }

    // 4. Write javac arguments to file
    let args_file = project_root.join("target/javac-args.txt");
    write_javac_args(
        &args_file,
        &src_root,
        &classes_dir,
        &manifest.package.java,
        &source_files,
    )?;

    // 5. Invoke javac
    let output = Command::new("javac")
        .arg(format!("@{}", args_file.display()))
        .current_dir(project_root)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                JargoError::JavacNotFound
            } else {
                e.into()
            }
        })?;

    // 6. Process output and rewrite error paths
    let success = output.status.success();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let errors = if !success {
        rewrite_error_paths(&stderr, &base_package)
    } else {
        Vec::new()
    };

    // 7. Copy resources if present
    if success {
        copy_resources(project_root)?;
    }

    Ok(CompileOutput { success, errors })
}

fn find_java_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    find_java_files_recursive(dir, &mut files)?;
    Ok(files)
}

fn find_java_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in
        fs::read_dir(dir).with_context(|| format!("failed to read directory {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            find_java_files_recursive(&path, files)?;
        } else if file_type.is_file() && path.extension().and_then(|s| s.to_str()) == Some("java") {
            files.push(path);
        }
    }

    Ok(())
}

fn write_javac_args(
    args_file: &Path,
    src_root: &Path,
    classes_dir: &Path,
    java_version: &str,
    source_files: &[PathBuf],
) -> Result<()> {
    let mut args = format!(
        "--release\n{}\n-d\n{}\n-sourcepath\n{}\n",
        java_version,
        classes_dir.display(),
        src_root.display()
    );

    // Add all source files
    for file in source_files {
        args.push_str(&format!("{}\n", file.display()));
    }

    fs::write(args_file, args)
        .with_context(|| format!("failed to write javac arguments to {}", args_file.display()))?;
    Ok(())
}

fn rewrite_error_paths(stderr: &str, base_package: &str) -> Vec<String> {
    // Replace "target/src-root/{base-package-path}/" with "src/"
    let package_path = base_package.replace('.', "/");
    let staged_prefix = format!("target/src-root/{}/", package_path);

    stderr
        .lines()
        .map(|line| line.replace(&staged_prefix, "src/"))
        .collect()
}

fn copy_resources(project_root: &Path) -> Result<()> {
    let resources = project_root.join("resources");
    if resources.exists() && resources.is_dir() {
        let classes_dir = project_root.join("target/classes");
        // Recursively copy resources/ contents into target/classes/
        copy_dir_recursive(&resources, &classes_dir)?;
    }
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    for entry in
        fs::read_dir(src).with_context(|| format!("failed to read directory {}", src.display()))?
    {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if ty.is_dir() {
            fs::create_dir_all(&dst_path)
                .with_context(|| format!("failed to create directory {}", dst_path.display()))?;
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_path_rewriting() {
        let stderr = "target/src-root/myapp/Main.java:5: error: ';' expected\n\
                      target/src-root/myapp/util/Helper.java:10: warning: unused variable";

        let rewritten = rewrite_error_paths(stderr, "myapp");

        assert_eq!(rewritten.len(), 2);
        assert_eq!(rewritten[0], "src/Main.java:5: error: ';' expected");
        assert_eq!(
            rewritten[1],
            "src/util/Helper.java:10: warning: unused variable"
        );
    }

    #[test]
    fn test_error_path_rewriting_nested_package() {
        let stderr = "target/src-root/com/example/app/Main.java:5: error: ';' expected";

        let rewritten = rewrite_error_paths(stderr, "com.example.app");

        assert_eq!(rewritten.len(), 1);
        assert_eq!(rewritten[0], "src/Main.java:5: error: ';' expected");
    }
}
