use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Dependency scope: determines which classpaths a dep appears on.
#[derive(Debug, Clone, PartialEq)]
pub enum Scope {
    Compile,
    Runtime,
}

impl Default for Scope {
    fn default() -> Self {
        Scope::Compile
    }
}

/// A dependency after normalization (parsed from either simple or expanded form).
#[derive(Debug, Clone)]
pub struct Dependency {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub scope: Scope,
    /// Only meaningful for lib projects. When true, consumers get this dep on their compile classpath.
    pub expose: bool,
}

/// Expanded dependency form: `{ version = "x", scope = "runtime", expose = true }`
#[derive(Debug, Serialize, Deserialize)]
pub struct DependencySpec {
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expose: Option<bool>,
}

/// Raw TOML value for a dependency entry. Handles both:
///   `"group:artifact" = "1.0"`  (Simple)
///   `"group:artifact" = { version = "1.0", scope = "runtime" }`  (Expanded)
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencyValue {
    Simple(String),
    Expanded(DependencySpec),
}

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

/// Represents the optional [run] section of Jargo.toml.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct RunConfig {
    #[serde(rename = "jvm-args", default, skip_serializing_if = "Vec::is_empty")]
    pub jvm_args: Vec<String>,
}

/// Top-level Jargo.toml structure for generation.
#[derive(Debug, Serialize, Deserialize)]
pub struct JargoToml {
    pub package: PackageManifest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunConfig>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub dependencies: HashMap<String, DependencyValue>,
    #[serde(rename = "dev-dependencies", default, skip_serializing_if = "HashMap::is_empty")]
    pub dev_dependencies: HashMap<String, DependencyValue>,
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
            run: None,
            dependencies: HashMap::new(),
            dev_dependencies: HashMap::new(),
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
            run: None,
            dependencies: HashMap::new(),
            dev_dependencies: HashMap::new(),
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

    /// Get JVM args from the [run] section, defaulting to empty.
    pub fn get_jvm_args(&self) -> &[String] {
        match &self.run {
            Some(run_config) => &run_config.jvm_args,
            None => &[],
        }
    }

    /// Parse and return the [dependencies] section as a normalized, sorted list.
    pub fn get_dependencies(&self) -> Result<Vec<Dependency>> {
        parse_dependency_map(&self.dependencies)
    }

    /// Parse and return the [dev-dependencies] section as a normalized, sorted list.
    pub fn get_dev_dependencies(&self) -> Result<Vec<Dependency>> {
        parse_dependency_map(&self.dev_dependencies)
    }
}

/// Parse a raw dependency map (from TOML) into a sorted, normalized list.
fn parse_dependency_map(map: &HashMap<String, DependencyValue>) -> Result<Vec<Dependency>> {
    let mut deps = Vec::with_capacity(map.len());

    for (coord, value) in map {
        let (group, artifact) = parse_coordinate(coord)?;
        let (version, scope, expose) = match value {
            DependencyValue::Simple(v) => (v.clone(), Scope::Compile, false),
            DependencyValue::Expanded(spec) => {
                let scope = match spec.scope.as_deref() {
                    None | Some("compile") => Scope::Compile,
                    Some("runtime") => Scope::Runtime,
                    Some(other) => bail!("unknown scope `{}` for `{}`", other, coord),
                };
                (spec.version.clone(), scope, spec.expose.unwrap_or(false))
            }
        };
        deps.push(Dependency { group, artifact, version, scope, expose });
    }

    // Sort for determinism — HashMap iteration order is unspecified.
    deps.sort_by(|a, b| (&a.group, &a.artifact).cmp(&(&b.group, &b.artifact)));
    Ok(deps)
}

/// Split `"groupId:artifactId"` into its two parts.
fn parse_coordinate(coord: &str) -> Result<(String, String)> {
    match coord.splitn(2, ':').collect::<Vec<_>>().as_slice() {
        [g, a] if !g.is_empty() && !a.is_empty() => Ok((g.to_string(), a.to_string())),
        _ => bail!(
            "invalid dependency coordinate `{}`: expected `groupId:artifactId`",
            coord
        ),
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

    #[test]
    fn test_no_deps_when_section_absent() {
        let toml_str = r#"
[package]
name = "test-app"
version = "1.0.0"
java = "21"
"#;
        let manifest: JargoToml = toml::from_str(toml_str).unwrap();
        assert!(manifest.get_dependencies().unwrap().is_empty());
        assert!(manifest.get_dev_dependencies().unwrap().is_empty());
    }

    #[test]
    fn test_simple_dependency() {
        let toml_str = r#"
[package]
name = "test-app"
version = "1.0.0"
java = "21"

[dependencies]
"org.apache.commons:commons-lang3" = "3.14.0"
"#;
        let manifest: JargoToml = toml::from_str(toml_str).unwrap();
        let deps = manifest.get_dependencies().unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].group, "org.apache.commons");
        assert_eq!(deps[0].artifact, "commons-lang3");
        assert_eq!(deps[0].version, "3.14.0");
        assert_eq!(deps[0].scope, Scope::Compile);
        assert!(!deps[0].expose);
    }

    #[test]
    fn test_expanded_dependency_runtime_scope() {
        let toml_str = r#"
[package]
name = "test-app"
version = "1.0.0"
java = "21"

[dependencies]
"org.postgresql:postgresql" = { version = "42.7.1", scope = "runtime" }
"#;
        let manifest: JargoToml = toml::from_str(toml_str).unwrap();
        let deps = manifest.get_dependencies().unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].version, "42.7.1");
        assert_eq!(deps[0].scope, Scope::Runtime);
        assert!(!deps[0].expose);
    }

    #[test]
    fn test_expanded_dependency_expose() {
        let toml_str = r#"
[package]
name = "my-lib"
version = "0.1.0"
type = "lib"
java = "21"

[dependencies]
"com.google.guava:guava" = { version = "33.0.0-jre", expose = true }
"#;
        let manifest: JargoToml = toml::from_str(toml_str).unwrap();
        let deps = manifest.get_dependencies().unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].artifact, "guava");
        assert_eq!(deps[0].scope, Scope::Compile);
        assert!(deps[0].expose);
    }

    #[test]
    fn test_dev_dependencies() {
        let toml_str = r#"
[package]
name = "test-app"
version = "1.0.0"
java = "21"

[dev-dependencies]
"org.assertj:assertj-core" = "3.25.1"
"#;
        let manifest: JargoToml = toml::from_str(toml_str).unwrap();
        assert!(manifest.get_dependencies().unwrap().is_empty());
        let dev_deps = manifest.get_dev_dependencies().unwrap();
        assert_eq!(dev_deps.len(), 1);
        assert_eq!(dev_deps[0].group, "org.assertj");
        assert_eq!(dev_deps[0].artifact, "assertj-core");
    }

    #[test]
    fn test_dependencies_sorted() {
        let toml_str = r#"
[package]
name = "test-app"
version = "1.0.0"
java = "21"

[dependencies]
"org.postgresql:postgresql" = "42.7.1"
"com.google.guava:guava" = "33.0.0-jre"
"org.apache.commons:commons-lang3" = "3.14.0"
"#;
        let manifest: JargoToml = toml::from_str(toml_str).unwrap();
        let deps = manifest.get_dependencies().unwrap();
        assert_eq!(deps.len(), 3);
        // Should be sorted by group then artifact
        assert_eq!(deps[0].group, "com.google.guava");
        assert_eq!(deps[1].group, "org.apache.commons");
        assert_eq!(deps[2].group, "org.postgresql");
    }

    #[test]
    fn test_invalid_coordinate_missing_colon() {
        let toml_str = r#"
[package]
name = "test-app"
version = "1.0.0"
java = "21"

[dependencies]
"badcoordinate" = "1.0.0"
"#;
        let manifest: JargoToml = toml::from_str(toml_str).unwrap();
        assert!(manifest.get_dependencies().is_err());
    }

    #[test]
    fn test_invalid_scope() {
        let toml_str = r#"
[package]
name = "test-app"
version = "1.0.0"
java = "21"

[dependencies]
"com.example:foo" = { version = "1.0.0", scope = "provided" }
"#;
        let manifest: JargoToml = toml::from_str(toml_str).unwrap();
        assert!(manifest.get_dependencies().is_err());
    }

    #[test]
    fn test_generated_manifest_has_no_dep_sections() {
        // New projects should not have [dependencies] or [dev-dependencies] sections in the TOML
        let toml = JargoToml::new_app("my-app");
        let s = toml.to_toml_string().unwrap();
        assert!(!s.contains("[dependencies]"));
        assert!(!s.contains("[dev-dependencies]"));
    }
}
