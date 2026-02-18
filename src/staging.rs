use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Create staging symlink structure for compilation.
/// Returns the path to target/src-root.
pub fn create_staging(project_root: &Path, base_package: &str) -> Result<PathBuf> {
    let target = project_root.join("target");
    let src_root = target.join("src-root");

    // Clean and recreate src-root
    if src_root.exists() {
        fs::remove_dir_all(&src_root)
            .with_context(|| format!("failed to remove {}", src_root.display()))?;
    }
    fs::create_dir_all(&src_root)
        .with_context(|| format!("failed to create {}", src_root.display()))?;

    // Convert base-package to path: "com.example.app" → "com/example/app"
    let package_path = base_package.replace('.', "/");
    let symlink_location = src_root.join(&package_path);

    // Create parent directories for symlink
    if let Some(parent) = symlink_location.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent directories for symlink"))?;
    }

    // Calculate relative path from symlink to src/
    // Count segments to determine how many "../" needed
    let segments: Vec<&str> = package_path.split('/').collect();
    let depth = segments.len();

    // Build relative path: depth+1 levels up, then "src"
    // For "myapp" (depth=1): ../../src
    // For "com/example/app" (depth=3): ../../../../src
    let mut relative_path = PathBuf::new();
    for _ in 0..=depth {
        relative_path.push("..");
    }
    relative_path.push("src");

    // Create symlink (Unix) or copy directory (Windows)
    create_symlink_or_copy(&relative_path, &symlink_location)?;

    Ok(src_root)
}

#[cfg(unix)]
fn create_symlink_or_copy(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link)
        .with_context(|| format!("failed to create symlink at {}", link.display()))?;
    Ok(())
}

#[cfg(windows)]
fn create_symlink_or_copy(source_relative: &Path, dest: &Path) -> Result<()> {
    // Windows fallback: resolve the relative path and recursively copy
    // This is less efficient but works without admin privileges
    let actual_src = dest
        .parent()
        .unwrap()
        .join(source_relative)
        .canonicalize()
        .with_context(|| "failed to resolve source directory")?;

    copy_dir_recursive(&actual_src, dest)
}

#[cfg(windows)]
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)
        .with_context(|| format!("failed to create directory {}", dst.display()))?;

    for entry in
        fs::read_dir(src).with_context(|| format!("failed to read directory {}", src.display()))?
    {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if ty.is_dir() {
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
    fn test_relative_path_calculation() {
        // Test single-level package
        let package = "myapp";
        let segments: Vec<&str> = package.split('/').collect();
        let depth = segments.len();
        assert_eq!(depth, 1);

        let mut relative_path = PathBuf::new();
        for _ in 0..=depth {
            relative_path.push("..");
        }
        relative_path.push("src");
        assert_eq!(relative_path, PathBuf::from("../../src"));
    }

    #[test]
    fn test_nested_package_path() {
        // Test nested package
        let package = "com.example.app";
        let package_path = package.replace('.', "/");
        assert_eq!(package_path, "com/example/app");

        let segments: Vec<&str> = package_path.split('/').collect();
        let depth = segments.len();
        assert_eq!(depth, 3);

        let mut relative_path = PathBuf::new();
        for _ in 0..=depth {
            relative_path.push("..");
        }
        relative_path.push("src");
        assert_eq!(relative_path, PathBuf::from("../../../../src"));
    }
}
