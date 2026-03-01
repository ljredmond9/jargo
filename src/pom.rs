use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::fs;
use std::path::Path;

/// A dependency extracted from POM or Gradle module metadata, ready for the resolver.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitiveDep {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub scope: TransitiveScope,
}

/// The scope of a transitive dependency as seen from its metadata file.
#[derive(Debug, Clone, PartialEq)]
pub enum TransitiveScope {
    /// Appears on both compile and runtime classpaths.
    Compile,
    /// Appears on the runtime classpath only.
    Runtime,
}

/// Parse a Maven POM file and return its direct dependency list.
///
/// Phase 1 constraints:
/// - No parent POM resolution
/// - No `${property}` substitution
/// - `<dependencyManagement>` section is skipped entirely
/// - Dependencies with no explicit version are skipped
/// - Optional dependencies are skipped
/// - Scopes `test`, `provided`, and `system` are excluded
pub fn parse_pom(path: &Path) -> Result<Vec<TransitiveDep>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read POM at {}", path.display()))?;
    parse_pom_str(&content)
        .with_context(|| format!("failed to parse POM at {}", path.display()))
}

fn parse_pom_str(xml: &str) -> Result<Vec<TransitiveDep>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut deps = Vec::new();
    // Stack of local element names, used to determine context.
    let mut stack: Vec<String> = Vec::new();

    // Fields collected for the dependency currently being parsed.
    let mut cur_group = String::new();
    let mut cur_artifact = String::new();
    let mut cur_version = String::new();
    let mut cur_scope = String::new();
    let mut cur_optional = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = local_name(&e.name());

                // Reset fields when we open a <dependency> in the main section.
                if name == "dependency" && in_main_dependencies(&stack) {
                    cur_group.clear();
                    cur_artifact.clear();
                    cur_version.clear();
                    cur_scope.clear();
                    cur_optional.clear();
                }

                stack.push(name);
            }

            Ok(Event::Text(e)) => {
                // Only collect text when inside a <dependency> in the main section.
                if in_dependency(&stack) {
                    if let Some(tag) = stack.last() {
                        let text = e.unescape().context("non-UTF8 text in POM")?.into_owned();
                        match tag.as_str() {
                            "groupId" => cur_group = text,
                            "artifactId" => cur_artifact = text,
                            "version" => cur_version = text,
                            "scope" => cur_scope = text,
                            "optional" => cur_optional = text,
                            _ => {}
                        }
                    }
                }
            }

            Ok(Event::End(e)) => {
                let name = local_name(&e.name());

                // Process completed <dependency> before popping it from the stack.
                if name == "dependency" && in_dependency(&stack) {
                    stack.pop();

                    // Skip optional.
                    if cur_optional == "true" {
                        continue;
                    }

                    // Resolve scope; skip excluded scopes.
                    let scope = match cur_scope.as_str() {
                        "" | "compile" => TransitiveScope::Compile,
                        "runtime" => TransitiveScope::Runtime,
                        _ => continue, // test, provided, system
                    };

                    // Skip unversioned deps (managed — Phase 2).
                    if cur_version.is_empty() || cur_group.is_empty() || cur_artifact.is_empty() {
                        continue;
                    }

                    deps.push(TransitiveDep {
                        group: cur_group.clone(),
                        artifact: cur_artifact.clone(),
                        version: cur_version.clone(),
                        scope,
                    });

                    continue; // stack already popped
                }

                stack.pop();
            }

            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow::anyhow!("XML parse error: {}", e)),
            _ => {}
        }
    }

    Ok(deps)
}

/// True when the stack shows we're inside the main `<dependencies>` section
/// (i.e., `<dependencies>` is present but `<dependencyManagement>` is not).
fn in_main_dependencies(stack: &[String]) -> bool {
    let has_deps = stack.iter().any(|s| s == "dependencies");
    let has_dm = stack.iter().any(|s| s == "dependencyManagement");
    has_deps && !has_dm
}

/// True when `<dependency>` is on the stack and we're in the main dependencies section.
fn in_dependency(stack: &[String]) -> bool {
    stack.iter().any(|s| s == "dependency") && in_main_dependencies(stack)
}

/// Extract the local name (stripping any namespace prefix) from a QName byte slice.
fn local_name(qname: &quick_xml::name::QName<'_>) -> String {
    String::from_utf8_lossy(qname.local_name().as_ref()).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
