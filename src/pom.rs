use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

// ---------------------------------------------------------------------------
// Phase 1 types (kept for backward compatibility)
// ---------------------------------------------------------------------------

/// A dependency extracted from POM or Gradle module metadata, ready for the resolver.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitiveDep {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub scope: TransitiveScope,
}

/// The scope of a transitive dependency as seen from its metadata file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransitiveScope {
    /// Appears on both compile and runtime classpaths.
    Compile,
    /// Appears on the runtime classpath only.
    Runtime,
}

// ---------------------------------------------------------------------------
// Phase 2 types (new)
// ---------------------------------------------------------------------------

/// A raw dependency entry as it appears in a POM file, before property
/// substitution or version resolution from `<dependencyManagement>`.
#[derive(Debug, Clone)]
pub struct RawDep {
    pub group: String,
    pub artifact: String,
    /// May be empty (managed) or contain `${...}` placeholders.
    pub version: String,
    /// May be empty (defaults to compile).
    pub scope: String,
    /// True if `<optional>true</optional>` was present.
    pub optional: bool,
}

/// An entry from `<dependencyManagement>` that provides a pinned version
/// (and optionally scope) for a dependency coordinate.
#[derive(Debug, Clone)]
pub struct ManagedEntry {
    /// May contain `${...}` placeholders.
    pub version: String,
    /// May be empty.
    pub scope: String,
}

/// Coordinates of a parent POM.
#[derive(Debug, Clone)]
pub struct ParentRef {
    pub group: String,
    pub artifact: String,
    pub version: String,
}

/// Everything extracted from a single POM file, without parent resolution or
/// property substitution applied.
pub struct ParsedPom {
    /// Project `<groupId>` (may be empty if inherited from parent).
    pub group: String,
    /// Project `<artifactId>`.
    pub artifact: String,
    /// Project `<version>` (may be empty or contain `${...}` placeholders).
    pub version: String,
    /// `<parent>` reference, if present.
    pub parent: Option<ParentRef>,
    /// Properties from `<properties>` section.
    pub properties: HashMap<String, String>,
    /// Version/scope overrides from `<dependencyManagement>`.
    pub managed: HashMap<(String, String), ManagedEntry>,
    /// Direct `<dependencies>` (raw; may have empty versions / `${...}` placeholders).
    /// Optional and excluded-scope entries are already filtered out.
    pub direct_deps: Vec<RawDep>,
}

// ---------------------------------------------------------------------------
// Public functions
// ---------------------------------------------------------------------------

/// Parse a Maven POM file and return its direct dependency list.
///
/// Phase 1 mode: no parent POM resolution, no `${property}` substitution.
/// Dependencies with no explicit version are skipped.
/// Scopes `test`, `provided`, and `system` are excluded.
///
/// Kept for testing and as a Phase 1 fallback.
#[allow(dead_code)]
pub fn parse_pom(path: &Path) -> Result<Vec<TransitiveDep>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read POM at {}", path.display()))?;
    parse_pom_str(&content).with_context(|| format!("failed to parse POM at {}", path.display()))
}

/// Parse a Maven POM file into raw form for Phase 2 processing.
///
/// Returns all data from the POM without resolving properties or parent chains.
/// The caller is responsible for following parent POMs, merging properties, and
/// filling in managed versions.
pub fn parse_pom_raw(path: &Path) -> Result<ParsedPom> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read POM at {}", path.display()))?;
    parse_pom_raw_str(&content)
        .with_context(|| format!("failed to parse POM at {}", path.display()))
}

// ---------------------------------------------------------------------------
// Private parsing functions
// ---------------------------------------------------------------------------

/// Phase 1 parse: delegates to the raw parser, then applies Phase 1 filtering.
fn parse_pom_str(xml: &str) -> Result<Vec<TransitiveDep>> {
    let raw = parse_pom_raw_str(xml)?;
    let deps = raw
        .direct_deps
        .iter()
        .filter(|d| !d.optional)
        .filter(|d| !d.group.is_empty() && !d.artifact.is_empty() && !d.version.is_empty())
        // Phase 1: skip property placeholders — they can't be resolved without parent chain
        .filter(|d| !d.version.starts_with('$'))
        .filter_map(|d| {
            let scope = match d.scope.as_str() {
                "" | "compile" => Some(TransitiveScope::Compile),
                "runtime" => Some(TransitiveScope::Runtime),
                _ => None,
            };
            scope.map(|s| TransitiveDep {
                group: d.group.clone(),
                artifact: d.artifact.clone(),
                version: d.version.clone(),
                scope: s,
            })
        })
        .collect();
    Ok(deps)
}

/// Parse the raw POM XML into a `ParsedPom`.
///
/// Uses a stack-based SAX-style parser to track element context without
/// building a full DOM tree.
fn parse_pom_raw_str(xml: &str) -> Result<ParsedPom> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut stack: Vec<String> = Vec::new();

    // Project-level fields
    let mut project_group = String::new();
    let mut project_artifact = String::new();
    let mut project_version = String::new();

    // Parent ref fields
    let mut parent_group = String::new();
    let mut parent_artifact = String::new();
    let mut parent_version = String::new();

    // Collected data
    let mut properties: HashMap<String, String> = HashMap::new();
    let mut managed: HashMap<(String, String), ManagedEntry> = HashMap::new();
    let mut direct_deps: Vec<RawDep> = Vec::new();

    // Current dependency being parsed (shared for direct and managed)
    let mut cur_group = String::new();
    let mut cur_artifact = String::new();
    let mut cur_version = String::new();
    let mut cur_scope = String::new();
    let mut cur_optional = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = local_name(&e.name());

                // Reset dep state when entering a <dependency> element.
                if name == "dependency" && has_tag(&stack, "dependencies") {
                    cur_group.clear();
                    cur_artifact.clear();
                    cur_version.clear();
                    cur_scope.clear();
                    cur_optional.clear();
                }

                stack.push(name);
            }

            Ok(Event::Text(e)) => {
                let text = e.unescape().context("non-UTF8 text in POM")?.into_owned();
                if let Some(tag) = stack.last() {
                    let tag = tag.clone();
                    if in_any_dep(&stack) {
                        // Inside <dependency> (direct or managed)
                        match tag.as_str() {
                            "groupId" => cur_group = text,
                            "artifactId" => cur_artifact = text,
                            "version" => cur_version = text,
                            "scope" => cur_scope = text,
                            "optional" => cur_optional = text,
                            _ => {}
                        }
                    } else if in_parent_element(&stack) {
                        // Inside <parent>
                        match tag.as_str() {
                            "groupId" => parent_group = text,
                            "artifactId" => parent_artifact = text,
                            "version" => parent_version = text,
                            _ => {}
                        }
                    } else if in_properties_element(&stack) && tag != "properties" {
                        // Inside <properties> — tag name is the property key
                        properties.insert(tag, text);
                    } else if is_project_direct_child(&stack) {
                        // Direct child of <project>
                        match tag.as_str() {
                            "groupId" => project_group = text,
                            "artifactId" => project_artifact = text,
                            "version" => project_version = text,
                            _ => {}
                        }
                    }
                }
            }

            Ok(Event::End(e)) => {
                let name = local_name(&e.name());

                // Commit completed <dependency> before popping it from the stack.
                if name == "dependency" && has_tag(&stack, "dependencies") {
                    let is_managed = has_tag(&stack, "dependencyManagement");
                    stack.pop();

                    let optional = cur_optional == "true";
                    if !optional && !cur_group.is_empty() && !cur_artifact.is_empty() {
                        if is_managed {
                            managed.insert(
                                (cur_group.clone(), cur_artifact.clone()),
                                ManagedEntry {
                                    version: cur_version.clone(),
                                    scope: cur_scope.clone(),
                                },
                            );
                        } else {
                            // Skip test/provided/system — these are not needed for transitive resolution
                            if !matches!(cur_scope.as_str(), "test" | "provided" | "system") {
                                direct_deps.push(RawDep {
                                    group: cur_group.clone(),
                                    artifact: cur_artifact.clone(),
                                    version: cur_version.clone(),
                                    scope: cur_scope.clone(),
                                    optional: false,
                                });
                            }
                        }
                    }

                    continue; // stack already popped
                }

                stack.pop();
            }

            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow::anyhow!("XML parse error: {}", e)),
            _ => {}
        }
    }

    let parent = if !parent_artifact.is_empty() {
        Some(ParentRef {
            group: parent_group,
            artifact: parent_artifact,
            version: parent_version,
        })
    } else {
        None
    };

    Ok(ParsedPom {
        group: project_group,
        artifact: project_artifact,
        version: project_version,
        parent,
        properties,
        managed,
        direct_deps,
    })
}

// ---------------------------------------------------------------------------
// Stack context helpers
// ---------------------------------------------------------------------------

/// True when any element with `tag` as its local name is present on the stack.
fn has_tag(stack: &[String], tag: &str) -> bool {
    stack.iter().any(|s| s == tag)
}

/// True when we're inside a `<dependency>` that is itself inside `<dependencies>`.
fn in_any_dep(stack: &[String]) -> bool {
    has_tag(stack, "dependency") && has_tag(stack, "dependencies")
}

/// True when we're inside `<parent>` but NOT inside a `<dependency>`.
fn in_parent_element(stack: &[String]) -> bool {
    has_tag(stack, "parent") && !has_tag(stack, "dependency")
}

/// True when we're inside `<properties>` but NOT inside a `<dependency>`.
fn in_properties_element(stack: &[String]) -> bool {
    has_tag(stack, "properties") && !has_tag(stack, "dependency")
}

/// True when the stack has exactly two elements (the project root and its direct child).
///
/// This identifies project-level fields like `<groupId>`, `<version>`, etc. that
/// are direct children of `<project>` rather than inside nested sections.
fn is_project_direct_child(stack: &[String]) -> bool {
    stack.len() == 2
}

/// Extract the local name (stripping any namespace prefix) from a QName byte slice.
fn local_name(qname: &quick_xml::name::QName<'_>) -> String {
    String::from_utf8_lossy(qname.local_name().as_ref()).into_owned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Phase 1 behavior (parse_pom_str) ---

    #[test]
    fn test_parse_simple_compile_dep() {
        let xml = r#"<?xml version="1.0"?>
<project>
  <dependencies>
    <dependency>
      <groupId>org.apache.commons</groupId>
      <artifactId>commons-lang3</artifactId>
      <version>3.14.0</version>
    </dependency>
  </dependencies>
</project>"#;
        let deps = parse_pom_str(xml).unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].group, "org.apache.commons");
        assert_eq!(deps[0].artifact, "commons-lang3");
        assert_eq!(deps[0].version, "3.14.0");
        assert_eq!(deps[0].scope, TransitiveScope::Compile);
    }

    #[test]
    fn test_parse_runtime_scope() {
        let xml = r#"<?xml version="1.0"?>
<project>
  <dependencies>
    <dependency>
      <groupId>org.postgresql</groupId>
      <artifactId>postgresql</artifactId>
      <version>42.7.1</version>
      <scope>runtime</scope>
    </dependency>
  </dependencies>
</project>"#;
        let deps = parse_pom_str(xml).unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].scope, TransitiveScope::Runtime);
    }

    #[test]
    fn test_skips_test_scope() {
        let xml = r#"<?xml version="1.0"?>
<project>
  <dependencies>
    <dependency>
      <groupId>junit</groupId>
      <artifactId>junit</artifactId>
      <version>4.13.2</version>
      <scope>test</scope>
    </dependency>
  </dependencies>
</project>"#;
        let deps = parse_pom_str(xml).unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn test_skips_provided_scope() {
        let xml = r#"<?xml version="1.0"?>
<project>
  <dependencies>
    <dependency>
      <groupId>javax.servlet</groupId>
      <artifactId>javax.servlet-api</artifactId>
      <version>4.0.1</version>
      <scope>provided</scope>
    </dependency>
  </dependencies>
</project>"#;
        let deps = parse_pom_str(xml).unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn test_skips_optional() {
        let xml = r#"<?xml version="1.0"?>
<project>
  <dependencies>
    <dependency>
      <groupId>com.example</groupId>
      <artifactId>optional-dep</artifactId>
      <version>1.0.0</version>
      <optional>true</optional>
    </dependency>
  </dependencies>
</project>"#;
        let deps = parse_pom_str(xml).unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn test_skips_dependency_management() {
        let xml = r#"<?xml version="1.0"?>
<project>
  <dependencyManagement>
    <dependencies>
      <dependency>
        <groupId>com.example</groupId>
        <artifactId>managed</artifactId>
        <version>1.0.0</version>
      </dependency>
    </dependencies>
  </dependencyManagement>
  <dependencies>
    <dependency>
      <groupId>com.example</groupId>
      <artifactId>real-dep</artifactId>
      <version>2.0.0</version>
    </dependency>
  </dependencies>
</project>"#;
        let deps = parse_pom_str(xml).unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].artifact, "real-dep");
    }

    #[test]
    fn test_skips_unversioned_dep() {
        let xml = r#"<?xml version="1.0"?>
<project>
  <dependencies>
    <dependency>
      <groupId>com.example</groupId>
      <artifactId>managed-no-version</artifactId>
    </dependency>
  </dependencies>
</project>"#;
        let deps = parse_pom_str(xml).unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn test_mixed_deps() {
        let xml = r#"<?xml version="1.0"?>
<project>
  <dependencies>
    <dependency>
      <groupId>com.google.guava</groupId>
      <artifactId>guava</artifactId>
      <version>33.0.0-jre</version>
    </dependency>
    <dependency>
      <groupId>org.slf4j</groupId>
      <artifactId>slf4j-api</artifactId>
      <version>2.0.9</version>
      <scope>runtime</scope>
    </dependency>
    <dependency>
      <groupId>org.junit.jupiter</groupId>
      <artifactId>junit-jupiter</artifactId>
      <version>5.10.1</version>
      <scope>test</scope>
    </dependency>
    <dependency>
      <groupId>com.example</groupId>
      <artifactId>optional-thing</artifactId>
      <version>1.0</version>
      <optional>true</optional>
    </dependency>
  </dependencies>
</project>"#;
        let deps = parse_pom_str(xml).unwrap();
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].artifact, "guava");
        assert_eq!(deps[0].scope, TransitiveScope::Compile);
        assert_eq!(deps[1].artifact, "slf4j-api");
        assert_eq!(deps[1].scope, TransitiveScope::Runtime);
    }

    #[test]
    fn test_pom_with_xml_namespace() {
        // Real Maven POMs typically have xmlns on the root element.
        let xml = r#"<?xml version="1.0"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <dependencies>
    <dependency>
      <groupId>com.example</groupId>
      <artifactId>foo</artifactId>
      <version>1.0.0</version>
    </dependency>
  </dependencies>
</project>"#;
        let deps = parse_pom_str(xml).unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].artifact, "foo");
    }

    #[test]
    fn test_empty_pom() {
        let xml = r#"<?xml version="1.0"?><project></project>"#;
        let deps = parse_pom_str(xml).unwrap();
        assert!(deps.is_empty());
    }

    // --- Phase 2 raw parsing (parse_pom_raw_str) ---

    #[test]
    fn test_raw_project_coordinates() {
        let xml = r#"<?xml version="1.0"?>
<project>
  <groupId>com.example</groupId>
  <artifactId>my-lib</artifactId>
  <version>2.3.4</version>
</project>"#;
        let parsed = parse_pom_raw_str(xml).unwrap();
        assert_eq!(parsed.group, "com.example");
        assert_eq!(parsed.artifact, "my-lib");
        assert_eq!(parsed.version, "2.3.4");
        assert!(parsed.parent.is_none());
    }

    #[test]
    fn test_raw_parent_ref() {
        let xml = r#"<?xml version="1.0"?>
<project>
  <artifactId>child</artifactId>
  <parent>
    <groupId>com.parent</groupId>
    <artifactId>parent-pom</artifactId>
    <version>3.0</version>
  </parent>
</project>"#;
        let parsed = parse_pom_raw_str(xml).unwrap();
        assert_eq!(parsed.artifact, "child");
        // groupId and version not specified — inherited from parent
        assert_eq!(parsed.group, "");
        assert_eq!(parsed.version, "");
        let parent = parsed.parent.unwrap();
        assert_eq!(parent.group, "com.parent");
        assert_eq!(parent.artifact, "parent-pom");
        assert_eq!(parent.version, "3.0");
    }

    #[test]
    fn test_raw_properties_section() {
        let xml = r#"<?xml version="1.0"?>
<project>
  <properties>
    <guava.version>33.0.0-jre</guava.version>
    <commons.version>3.14.0</commons.version>
  </properties>
</project>"#;
        let parsed = parse_pom_raw_str(xml).unwrap();
        assert_eq!(
            parsed.properties.get("guava.version"),
            Some(&"33.0.0-jre".to_string())
        );
        assert_eq!(
            parsed.properties.get("commons.version"),
            Some(&"3.14.0".to_string())
        );
    }

    #[test]
    fn test_raw_dependency_management() {
        let xml = r#"<?xml version="1.0"?>
<project>
  <dependencyManagement>
    <dependencies>
      <dependency>
        <groupId>com.example</groupId>
        <artifactId>foo</artifactId>
        <version>1.2.3</version>
      </dependency>
      <dependency>
        <groupId>com.example</groupId>
        <artifactId>bar</artifactId>
        <version>4.5.6</version>
        <scope>runtime</scope>
      </dependency>
    </dependencies>
  </dependencyManagement>
</project>"#;
        let parsed = parse_pom_raw_str(xml).unwrap();
        assert!(parsed.direct_deps.is_empty());
        let foo_key = ("com.example".to_string(), "foo".to_string());
        assert_eq!(
            parsed.managed.get(&foo_key).map(|m| m.version.as_str()),
            Some("1.2.3")
        );
        let bar_key = ("com.example".to_string(), "bar".to_string());
        let bar = parsed.managed.get(&bar_key).unwrap();
        assert_eq!(bar.version, "4.5.6");
        assert_eq!(bar.scope, "runtime");
    }

    #[test]
    fn test_raw_dep_keeps_property_placeholder() {
        let xml = r#"<?xml version="1.0"?>
<project>
  <dependencies>
    <dependency>
      <groupId>com.example</groupId>
      <artifactId>foo</artifactId>
      <version>${foo.version}</version>
    </dependency>
  </dependencies>
</project>"#;
        let parsed = parse_pom_raw_str(xml).unwrap();
        assert_eq!(parsed.direct_deps.len(), 1);
        assert_eq!(parsed.direct_deps[0].version, "${foo.version}");
    }

    #[test]
    fn test_raw_dep_empty_version_for_managed() {
        // Dep with no version (to be filled from dependencyManagement)
        let xml = r#"<?xml version="1.0"?>
<project>
  <dependencyManagement>
    <dependencies>
      <dependency>
        <groupId>com.example</groupId>
        <artifactId>foo</artifactId>
        <version>1.0.0</version>
      </dependency>
    </dependencies>
  </dependencyManagement>
  <dependencies>
    <dependency>
      <groupId>com.example</groupId>
      <artifactId>foo</artifactId>
    </dependency>
  </dependencies>
</project>"#;
        let raw = parse_pom_raw_str(xml).unwrap();
        // Raw parser keeps the unversioned dep (Phase 2 will fill it in)
        assert_eq!(raw.direct_deps.len(), 1);
        assert_eq!(raw.direct_deps[0].version, "");
        let key = ("com.example".to_string(), "foo".to_string());
        assert_eq!(
            raw.managed.get(&key).map(|m| m.version.as_str()),
            Some("1.0.0")
        );
        // Phase 1 parse_pom_str must skip the unversioned dep
        let phase1 = parse_pom_str(xml).unwrap();
        assert!(phase1.is_empty());
    }

    #[test]
    fn test_raw_property_in_version_skipped_by_phase1() {
        let xml = r#"<?xml version="1.0"?>
<project>
  <properties>
    <foo.version>2.0.0</foo.version>
  </properties>
  <dependencies>
    <dependency>
      <groupId>com.example</groupId>
      <artifactId>foo</artifactId>
      <version>${foo.version}</version>
    </dependency>
  </dependencies>
</project>"#;
        // Phase 1 skips property-versioned deps
        let phase1 = parse_pom_str(xml).unwrap();
        assert!(phase1.is_empty());
        // Raw keeps them
        let raw = parse_pom_raw_str(xml).unwrap();
        assert_eq!(raw.direct_deps.len(), 1);
        assert_eq!(raw.direct_deps[0].version, "${foo.version}");
    }

    #[test]
    fn test_raw_groupid_not_confused_with_parent_groupid() {
        let xml = r#"<?xml version="1.0"?>
<project>
  <groupId>com.example.child</groupId>
  <parent>
    <groupId>com.example</groupId>
    <artifactId>parent</artifactId>
    <version>1.0</version>
  </parent>
</project>"#;
        let parsed = parse_pom_raw_str(xml).unwrap();
        // Child overrides parent groupId
        assert_eq!(parsed.group, "com.example.child");
        assert_eq!(parsed.parent.unwrap().group, "com.example");
    }
}
