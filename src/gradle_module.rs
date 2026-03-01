use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

use crate::pom::{TransitiveDep, TransitiveScope};

// --- Serde structs for Gradle module JSON ---

#[derive(Deserialize)]
struct GradleModule {
    #[serde(default)]
    variants: Vec<Variant>,
}

#[derive(Deserialize)]
struct Variant {
    name: String,
    #[serde(default)]
    dependencies: Vec<GradleDep>,
}

#[derive(Deserialize)]
struct GradleDep {
    group: String,
    module: String,
    version: Option<GradleVersion>,
}

#[derive(Deserialize)]
struct GradleVersion {
    /// `strictly` pins an exact version.
    strictly: Option<String>,
    /// `requires` is the normal version constraint.
    requires: Option<String>,
    /// `prefers` is a soft preference, used as last resort.
    prefers: Option<String>,
}

impl GradleVersion {
    /// Return the most specific version string available.
    fn resolve(&self) -> Option<String> {
        self.strictly
            .clone()
            .or_else(|| self.requires.clone())
            .or_else(|| self.prefers.clone())
    }
}

/// Parse a Gradle `.module` file and return its dependencies.
///
/// Recognises two categories of variants:
/// - `*ApiElements` (e.g. `apiElements`, `jvmApiElements`) → `Compile` scope
/// - `*RuntimeElements` (e.g. `runtimeElements`, `jvmRuntimeElements`) → `Runtime` scope
///
/// All other variants (sources, javadoc, etc.) are ignored.
/// Dependencies with no resolvable version are skipped.
pub fn parse_module(path: &Path) -> Result<Vec<TransitiveDep>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read .module at {}", path.display()))?;
    let module: GradleModule = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse .module at {}", path.display()))?;

    let mut deps: Vec<TransitiveDep> = Vec::new();

    for variant in &module.variants {
        let scope = match classify_variant(&variant.name) {
            Some(s) => s,
            None => continue, // javadoc, sources, etc. — skip
        };

        for dep in &variant.dependencies {
            let version = match &dep.version {
                Some(v) => match v.resolve() {
                    Some(ver) => ver,
                    None => continue, // no usable version — skip
                },
                None => continue,
            };

            deps.push(TransitiveDep {
                group: dep.group.clone(),
                artifact: dep.module.clone(),
                version,
                scope: scope.clone(),
            });
        }
    }

    // Deduplicate: if a dep appears in both apiElements and runtimeElements,
    // keep the Compile entry (higher scope wins).
    dedup_by_scope(deps)
}

/// Classify a variant name into a scope, returning `None` for irrelevant variants.
///
/// - `apiElements` or anything ending in `ApiElements` → `Compile`
///   (covers `jvmApiElements`, `releaseApiElements`, etc.)
/// - `runtimeElements` or anything ending in `RuntimeElements` → `Runtime`
///   (covers `jvmRuntimeElements`, `releaseRuntimeElements`, etc.)
fn classify_variant(name: &str) -> Option<TransitiveScope> {
    if name == "apiElements" || name.ends_with("ApiElements") {
        Some(TransitiveScope::Compile)
    } else if name == "runtimeElements" || name.ends_with("RuntimeElements") {
        Some(TransitiveScope::Runtime)
    } else {
        None
    }
}

/// Deduplicate the dep list by `(group, artifact)`, keeping the highest scope
/// (Compile > Runtime) for any duplicates.
fn dedup_by_scope(deps: Vec<TransitiveDep>) -> Result<Vec<TransitiveDep>> {
    use std::collections::HashMap;

    let mut map: HashMap<(String, String), TransitiveDep> = HashMap::new();

    for dep in deps {
        let key = (dep.group.clone(), dep.artifact.clone());
        match map.get(&key) {
            // Compile beats Runtime — only upgrade, never downgrade.
            Some(existing) if existing.scope == TransitiveScope::Compile => {}
            _ => {
                map.insert(key, dep);
            }
        }
    }

    let mut result: Vec<TransitiveDep> = map.into_values().collect();
    result.sort_by(|a, b| (&a.group, &a.artifact).cmp(&(&b.group, &b.artifact)));
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Vec<TransitiveDep> {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        fs::write(tmp.path(), json).unwrap();
        parse_module(tmp.path()).unwrap()
    }

    #[test]
    fn test_api_elements_is_compile_scope() {
        let json = r#"{
            "formatVersion": "1.1",
            "variants": [
                {
                    "name": "apiElements",
                    "dependencies": [
                        {
                            "group": "com.google.code.findbugs",
                            "module": "jsr305",
                            "version": { "requires": "3.0.2" }
                        }
                    ]
                }
            ]
        }"#;
        let deps = parse(json);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].group, "com.google.code.findbugs");
        assert_eq!(deps[0].artifact, "jsr305");
        assert_eq!(deps[0].version, "3.0.2");
        assert_eq!(deps[0].scope, TransitiveScope::Compile);
    }

    #[test]
    fn test_runtime_elements_is_runtime_scope() {
        let json = r#"{
            "formatVersion": "1.1",
            "variants": [
                {
                    "name": "runtimeElements",
                    "dependencies": [
                        {
                            "group": "org.slf4j",
                            "module": "slf4j-api",
                            "version": { "requires": "2.0.9" }
                        }
                    ]
                }
            ]
        }"#;
        let deps = parse(json);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].scope, TransitiveScope::Runtime);
    }

    #[test]
    fn test_unknown_variants_ignored() {
        let json = r#"{
            "formatVersion": "1.1",
            "variants": [
                {
                    "name": "javadocElements",
                    "dependencies": [
                        { "group": "com.example", "module": "foo", "version": { "requires": "1.0" } }
                    ]
                },
                {
                    "name": "sourcesElements",
                    "dependencies": [
                        { "group": "com.example", "module": "bar", "version": { "requires": "1.0" } }
                    ]
                }
            ]
        }"#;
        let deps = parse(json);
        assert!(deps.is_empty());
    }

    #[test]
    fn test_compile_beats_runtime_on_dedup() {
        // Same dep appears in both apiElements and runtimeElements; Compile wins.
        let json = r#"{
            "formatVersion": "1.1",
            "variants": [
                {
                    "name": "apiElements",
                    "dependencies": [
                        { "group": "com.example", "module": "foo", "version": { "requires": "1.0" } }
                    ]
                },
                {
                    "name": "runtimeElements",
                    "dependencies": [
                        { "group": "com.example", "module": "foo", "version": { "requires": "1.0" } }
                    ]
                }
            ]
        }"#;
        let deps = parse(json);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].scope, TransitiveScope::Compile);
    }

    #[test]
    fn test_jvm_prefixed_variants() {
        // Kotlin multiplatform libraries use jvmApiElements / jvmRuntimeElements.
        let json = r#"{
            "formatVersion": "1.1",
            "variants": [
                {
                    "name": "jvmApiElements",
                    "dependencies": [
                        { "group": "org.jetbrains.kotlin", "module": "kotlin-stdlib", "version": { "requires": "1.9.0" } }
                    ]
                },
                {
                    "name": "jvmRuntimeElements",
                    "dependencies": [
                        { "group": "org.example", "module": "runtime-only", "version": { "requires": "2.0.0" } }
                    ]
                }
            ]
        }"#;
        let deps = parse(json);
        assert_eq!(deps.len(), 2);
        let kotlin = deps.iter().find(|d| d.artifact == "kotlin-stdlib").unwrap();
        assert_eq!(kotlin.scope, TransitiveScope::Compile);
        let rt = deps.iter().find(|d| d.artifact == "runtime-only").unwrap();
        assert_eq!(rt.scope, TransitiveScope::Runtime);
    }

    #[test]
    fn test_strictly_version_takes_priority() {
        let json = r#"{
            "formatVersion": "1.1",
            "variants": [
                {
                    "name": "apiElements",
                    "dependencies": [
                        {
                            "group": "com.example",
                            "module": "foo",
                            "version": { "strictly": "2.0.0", "requires": "1.0.0" }
                        }
                    ]
                }
            ]
        }"#;
        let deps = parse(json);
        assert_eq!(deps[0].version, "2.0.0");
    }

    #[test]
    fn test_skips_dep_with_no_version() {
        let json = r#"{
            "formatVersion": "1.1",
            "variants": [
                {
                    "name": "apiElements",
                    "dependencies": [
                        { "group": "com.example", "module": "foo" }
                    ]
                }
            ]
        }"#;
        let deps = parse(json);
        assert!(deps.is_empty());
    }

    #[test]
    fn test_empty_module() {
        let json = r#"{ "formatVersion": "1.1", "variants": [] }"#;
        let deps = parse(json);
        assert!(deps.is_empty());
    }
}
