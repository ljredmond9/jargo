use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A single resolved dependency entry in Jargo.lock.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LockedDependency {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub sha256: String,
}

/// The full contents of a Jargo.lock file.
///
/// TOML format uses `[[dependency]]` array-of-tables:
/// ```toml
/// [[dependency]]
/// group = "com.google.guava"
/// artifact = "guava"
/// version = "33.0.0-jre"
/// sha256 = "abcdef..."
/// ```
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct LockFile {
    #[serde(default)]
    pub dependency: Vec<LockedDependency>,
}

impl LockFile {
    pub fn new() -> Self {
        Self::default()
    }

    /// Read and parse a Jargo.lock file.
    pub fn read(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))
    }

    /// Serialize and write this lock file to disk.
    pub fn write(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .context("failed to serialize lock file")?;
        std::fs::write(path, content)
            .with_context(|| format!("failed to write {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_empty_lockfile_round_trip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("Jargo.lock");

        let lock = LockFile::new();
        lock.write(&path).unwrap();

        let loaded = LockFile::read(&path).unwrap();
        assert!(loaded.dependency.is_empty());
    }

    #[test]
    fn test_lockfile_round_trip_with_entries() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("Jargo.lock");

        let lock = LockFile {
            dependency: vec![
                LockedDependency {
                    group: "com.google.guava".to_string(),
                    artifact: "guava".to_string(),
                    version: "33.0.0-jre".to_string(),
                    sha256: "abc123".to_string(),
                },
                LockedDependency {
                    group: "org.apache.commons".to_string(),
                    artifact: "commons-lang3".to_string(),
                    version: "3.14.0".to_string(),
                    sha256: "def456".to_string(),
                },
            ],
        };

        lock.write(&path).unwrap();
        let loaded = LockFile::read(&path).unwrap();

        assert_eq!(loaded.dependency.len(), 2);
        assert_eq!(loaded.dependency[0], lock.dependency[0]);
        assert_eq!(loaded.dependency[1], lock.dependency[1]);
    }

    #[test]
    fn test_lockfile_toml_format() {
        let lock = LockFile {
            dependency: vec![LockedDependency {
                group: "com.example".to_string(),
                artifact: "foo".to_string(),
                version: "1.0.0".to_string(),
                sha256: "deadbeef".to_string(),
            }],
        };

        let s = toml::to_string_pretty(&lock).unwrap();
        assert!(s.contains("[[dependency]]"));
        assert!(s.contains("group = \"com.example\""));
        assert!(s.contains("artifact = \"foo\""));
        assert!(s.contains("version = \"1.0.0\""));
        assert!(s.contains("sha256 = \"deadbeef\""));
    }

    #[test]
    fn test_read_nonexistent_file_errors() {
        let result = LockFile::read(Path::new("/nonexistent/Jargo.lock"));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_lock_toml_directly() {
        let toml_str = r#"
[[dependency]]
group = "com.google.guava"
artifact = "guava"
version = "33.0.0-jre"
sha256 = "abc123"

[[dependency]]
group = "com.google.code.findbugs"
artifact = "jsr305"
version = "3.0.2"
sha256 = "def456"
"#;
        let lock: LockFile = toml::from_str(toml_str).unwrap();
        assert_eq!(lock.dependency.len(), 2);
        assert_eq!(lock.dependency[0].artifact, "guava");
        assert_eq!(lock.dependency[1].artifact, "jsr305");
    }
}
