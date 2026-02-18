use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Represents the [package] section of Jargo.toml.
#[derive(Debug, Serialize, Deserialize)]
pub struct PackageManifest {
    pub name: String,
    pub version: String,
    #[serde(rename = "type", default = "default_type")]
    pub project_type: String,
    pub java: String,
    #[serde(rename = "base-package", skip_serializing_if = "Option::is_none")]
    pub base_package: Option<String>,
    #[serde(rename = "main-class", skip_serializing_if = "Option::is_none")]
    pub main_class: Option<String>,
}

fn default_type() -> String {
    "app".to_string()
}

/// Top-level Jargo.toml structure for generation.
#[derive(Debug, Serialize, Deserialize)]
pub struct JargoToml {
    pub package: PackageManifest,
}

impl JargoToml {
    pub fn new_app(name: &str) -> Self {
        Self {
            package: PackageManifest {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                project_type: "app".to_string(),
                java: "21".to_string(),
                base_package: None,
                main_class: None,
            },
        }
    }

    pub fn new_lib(name: &str, base_package: &str) -> Self {
        Self {
            package: PackageManifest {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                project_type: "lib".to_string(),
                java: "21".to_string(),
                base_package: Some(base_package.to_string()),
                main_class: None,
            },
        }
    }

    pub fn to_toml_string(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Load and parse a Jargo.toml file.
    pub fn from_file(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let manifest: JargoToml = toml::from_str(&content)?;
        Ok(manifest)
    }

    /// Get the base package, using the derived name if not explicitly set.
    pub fn get_base_package(&self) -> String {
        self.package
            .base_package
            .clone()
            .unwrap_or_else(|| derive_base_package(&self.package.name))
    }

    /// Get the main class name, defaulting to "Main" if not set.
    pub fn get_main_class(&self) -> String {
        self.package
            .main_class
            .clone()
            .unwrap_or_else(|| "Main".to_string())
    }

    /// Check if this is an app project.
    pub fn is_app(&self) -> bool {
        self.package.project_type == "app"
    }
}

/// Derive base-package name from project name by stripping hyphens.
pub fn derive_base_package(name: &str) -> String {
    name.replace('-', "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_base_package() {
        assert_eq!(derive_base_package("my-app"), "myapp");
        assert_eq!(derive_base_package("hello"), "hello");
        assert_eq!(derive_base_package("my-cool-lib"), "mycoollib");
    }

    #[test]
    fn test_app_toml_generation() {
        let toml = JargoToml::new_app("my-app");
        let s = toml.to_toml_string().unwrap();
        assert!(s.contains("name = \"my-app\""));
        assert!(s.contains("type = \"app\""));
        assert!(s.contains("java = \"21\""));
        assert!(!s.contains("base-package"));
    }

    #[test]
    fn test_lib_toml_generation() {
        let toml = JargoToml::new_lib("my-lib", "mylib");
        let s = toml.to_toml_string().unwrap();
        assert!(s.contains("name = \"my-lib\""));
        assert!(s.contains("type = \"lib\""));
        assert!(s.contains("base-package = \"mylib\""));
    }

    #[test]
    fn test_toml_parsing() {
        let toml_str = r#"
[package]
name = "test-app"
version = "1.0.0"
type = "app"
java = "17"
"#;
        let manifest: JargoToml = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.package.name, "test-app");
        assert_eq!(manifest.package.version, "1.0.0");
        assert_eq!(manifest.package.project_type, "app");
        assert_eq!(manifest.package.java, "17");
        assert!(manifest.package.base_package.is_none());
    }

    #[test]
    fn test_get_base_package() {
        let toml = JargoToml::new_app("my-app");
        assert_eq!(toml.get_base_package(), "myapp");

        let toml = JargoToml::new_lib("my-lib", "com.example.mylib");
        assert_eq!(toml.get_base_package(), "com.example.mylib");
    }

    #[test]
    fn test_get_main_class() {
        let toml = JargoToml::new_app("my-app");
        assert_eq!(toml.get_main_class(), "Main");
    }

    #[test]
    fn test_is_app() {
        let toml = JargoToml::new_app("my-app");
        assert!(toml.is_app());

        let toml = JargoToml::new_lib("my-lib", "mylib");
        assert!(!toml.is_app());
    }

    #[test]
    fn test_default_type() {
        let toml_str = r#"
[package]
name = "test-app"
version = "1.0.0"
java = "21"
"#;
        let manifest: JargoToml = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.package.project_type, "app");
    }
}
