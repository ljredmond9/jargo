use serde::Serialize;

/// Represents the [package] section of Jargo.toml.
#[derive(Debug, Serialize)]
pub struct PackageManifest {
    pub name: String,
    pub version: String,
    #[serde(rename = "type")]
    pub project_type: String,
    pub java: String,
    #[serde(rename = "base-package", skip_serializing_if = "Option::is_none")]
    pub base_package: Option<String>,
}

/// Top-level Jargo.toml structure for generation.
#[derive(Debug, Serialize)]
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
            },
        }
    }

    pub fn to_toml_string(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
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
}
