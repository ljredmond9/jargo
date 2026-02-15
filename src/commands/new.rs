use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use crate::errors::JargoError;
use crate::manifest::{self, JargoToml};

/// Validate a project name: must be non-empty, start with a letter,
/// and contain only ASCII lowercase letters, digits, and hyphens.
pub fn validate_name(name: &str) -> Result<(), JargoError> {
    if name.is_empty() {
        return Err(JargoError::InvalidName(
            name.to_string(),
            "name cannot be empty".to_string(),
        ));
    }

    let first = name.chars().next().unwrap();
    if !first.is_ascii_alphabetic() {
        return Err(JargoError::InvalidName(
            name.to_string(),
            "must start with a letter".to_string(),
        ));
    }

    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(JargoError::InvalidName(
            name.to_string(),
            "must contain only lowercase letters, digits, and hyphens".to_string(),
        ));
    }

    if name.ends_with('-') {
        return Err(JargoError::InvalidName(
            name.to_string(),
            "must not end with a hyphen".to_string(),
        ));
    }

    Ok(())
}

/// Execute `jargo new <name>`.
pub fn exec(name: &str, is_lib: bool) -> Result<()> {
    validate_name(name)?;

    let path = Path::new(name);
    if path.exists() {
        return Err(JargoError::ProjectExists(name.to_string()).into());
    }

    fs::create_dir(path).with_context(|| format!("failed to create directory `{name}`"))?;

    scaffold(path, name, is_lib)?;

    // Initialize git repository
    let _ = Command::new("git")
        .arg("init")
        .current_dir(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    let kind = if is_lib { "lib" } else { "app" };
    println!("    Created {kind} `{name}` package");

    Ok(())
}

/// Shared scaffolding logic used by both `new` and `init`.
pub fn scaffold(project_dir: &Path, name: &str, is_lib: bool) -> Result<()> {
    let base_package = manifest::derive_base_package(name);

    // Generate Jargo.toml
    let toml = if is_lib {
        JargoToml::new_lib(name, &base_package)
    } else {
        JargoToml::new_app(name)
    };
    let toml_content = toml
        .to_toml_string()
        .context("failed to serialize Jargo.toml")?;
    fs::write(project_dir.join("Jargo.toml"), toml_content)?;

    // Create directories
    fs::create_dir(project_dir.join("src"))?;
    fs::create_dir(project_dir.join("test"))?;

    // Generate source files
    if is_lib {
        fs::write(
            project_dir.join("src/Lib.java"),
            generate_lib_java(&base_package, name),
        )?;
        fs::write(
            project_dir.join("test/LibTest.java"),
            generate_lib_test_java(&base_package, name),
        )?;
    } else {
        fs::write(
            project_dir.join("src/Main.java"),
            generate_main_java(&base_package),
        )?;
        fs::write(
            project_dir.join("test/MainTest.java"),
            generate_main_test_java(&base_package),
        )?;
    }

    // Generate .gitignore
    fs::write(project_dir.join(".gitignore"), "target/\n")?;

    Ok(())
}

fn generate_main_java(base_package: &str) -> String {
    format!(
        r#"package {base_package};

public class Main {{
    public static void main(String[] args) {{
        System.out.println("Hello, World!");
    }}
}}
"#
    )
}

fn generate_main_test_java(base_package: &str) -> String {
    format!(
        r#"package {base_package};

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class MainTest {{
    @Test
    void testMain() {{
        // TODO: add tests
        assertTrue(true);
    }}
}}
"#
    )
}

fn generate_lib_java(base_package: &str, name: &str) -> String {
    format!(
        r#"package {base_package};

public class Lib {{
    public static String greeting() {{
        return "Hello from {name}!";
    }}
}}
"#
    )
}

fn generate_lib_test_java(base_package: &str, name: &str) -> String {
    format!(
        r#"package {base_package};

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class LibTest {{
    @Test
    void testGreeting() {{
        assertEquals("Hello from {name}!", Lib.greeting());
    }}
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_names() {
        assert!(validate_name("my-app").is_ok());
        assert!(validate_name("hello").is_ok());
        assert!(validate_name("app2").is_ok());
        assert!(validate_name("a").is_ok());
    }

    #[test]
    fn test_invalid_names() {
        assert!(validate_name("").is_err());
        assert!(validate_name("-app").is_err());
        assert!(validate_name("2app").is_err());
        assert!(validate_name("My-App").is_err());
        assert!(validate_name("my_app").is_err());
        assert!(validate_name("my app").is_err());
        assert!(validate_name("app-").is_err());
    }
}
