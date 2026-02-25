use std::process::Command;
use tempfile::TempDir;

fn jargo_bin() -> String {
    env!("CARGO_BIN_EXE_jargo").to_string()
}

#[test]
fn test_build_simple_app() {
    let temp = TempDir::new().unwrap();
    let project_path = temp.path().join("test-app");

    // Create project with jargo new
    let output = Command::new(jargo_bin())
        .args(&["new", "test-app"])
        .current_dir(temp.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "jargo new failed");

    // Build the project
    let output = Command::new(jargo_bin())
        .arg("build")
        .current_dir(&project_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "jargo build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify JAR exists
    assert!(project_path.join("target/test-app.jar").exists());

    // Verify stdout contains compilation message
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Compiling test-app"));
    assert!(stdout.contains("Finished JAR at"));
}

#[test]
fn test_jar_is_runnable() {
    let temp = TempDir::new().unwrap();
    let project_path = temp.path().join("test-app");

    // Create and build project
    Command::new(jargo_bin())
        .args(&["new", "test-app"])
        .current_dir(temp.path())
        .output()
        .unwrap();

    Command::new(jargo_bin())
        .arg("build")
        .current_dir(&project_path)
        .output()
        .unwrap();

    // Run the JAR with java
    let jar_output = Command::new("java")
        .args(&["-jar", "target/test-app.jar"])
        .current_dir(&project_path)
        .output()
        .unwrap();

    assert!(
        jar_output.status.success(),
        "java -jar failed: {}",
        String::from_utf8_lossy(&jar_output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&jar_output.stdout).trim(),
        "Hello, World!"
    );
}

#[test]
fn test_clean_removes_target() {
    let temp = TempDir::new().unwrap();
    let project_path = temp.path().join("test-app");

    // Setup
    Command::new(jargo_bin())
        .args(&["new", "test-app"])
        .current_dir(temp.path())
        .output()
        .unwrap();

    Command::new(jargo_bin())
        .arg("build")
        .current_dir(&project_path)
        .output()
        .unwrap();

    assert!(project_path.join("target").exists());

    // Clean
    let output = Command::new(jargo_bin())
        .arg("clean")
        .current_dir(&project_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(!project_path.join("target").exists());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Removed target directory"));
}

#[test]
fn test_clean_when_no_target() {
    let temp = TempDir::new().unwrap();
    let project_path = temp.path().join("test-app");

    Command::new(jargo_bin())
        .args(&["new", "test-app"])
        .current_dir(temp.path())
        .output()
        .unwrap();

    // Clean without building first
    let output = Command::new(jargo_bin())
        .arg("clean")
        .current_dir(&project_path)
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Nothing to clean"));
}

#[test]
fn test_build_lib_project() {
    let temp = TempDir::new().unwrap();
    let project_path = temp.path().join("test-lib");

    // Create lib project
    Command::new(jargo_bin())
        .args(&["new", "--lib", "test-lib"])
        .current_dir(temp.path())
        .output()
        .unwrap();

    // Build the project
    let output = Command::new(jargo_bin())
        .arg("build")
        .current_dir(&project_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "jargo build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify JAR exists
    assert!(project_path.join("target/test-lib.jar").exists());
}

#[test]
fn test_rebuild_after_clean() {
    let temp = TempDir::new().unwrap();
    let project_path = temp.path().join("test-app");

    Command::new(jargo_bin())
        .args(&["new", "test-app"])
        .current_dir(temp.path())
        .output()
        .unwrap();

    // Build
    Command::new(jargo_bin())
        .arg("build")
        .current_dir(&project_path)
        .output()
        .unwrap();

    // Clean
    Command::new(jargo_bin())
        .arg("clean")
        .current_dir(&project_path)
        .output()
        .unwrap();

    // Build again
    let output = Command::new(jargo_bin())
        .arg("build")
        .current_dir(&project_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(project_path.join("target/test-app.jar").exists());
}

#[test]
fn test_run_simple_app() {
    let temp = TempDir::new().unwrap();
    let project_path = temp.path().join("test-app");

    // Create project
    Command::new(jargo_bin())
        .args(&["new", "test-app"])
        .current_dir(temp.path())
        .output()
        .unwrap();

    // Run the project
    let output = Command::new(jargo_bin())
        .arg("run")
        .current_dir(&project_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "jargo run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Compiling test-app"));
    assert!(stdout.contains("Running test-app"));
    assert!(stdout.contains("Hello, World!"));
}

#[test]
fn test_run_lib_project_fails() {
    let temp = TempDir::new().unwrap();
    let project_path = temp.path().join("test-lib");

    // Create lib project
    Command::new(jargo_bin())
        .args(&["new", "--lib", "test-lib"])
        .current_dir(temp.path())
        .output()
        .unwrap();

    // Run should fail for lib project
    let output = Command::new(jargo_bin())
        .arg("run")
        .current_dir(&project_path)
        .output()
        .unwrap();

    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("app"));
}

#[test]
fn test_run_with_jvm_args() {
    let temp = TempDir::new().unwrap();
    let project_path = temp.path().join("test-app");

    // Create project
    Command::new(jargo_bin())
        .args(&["new", "test-app"])
        .current_dir(temp.path())
        .output()
        .unwrap();

    // Add [run] section with jvm-args to Jargo.toml
    let manifest_path = project_path.join("Jargo.toml");
    let content = std::fs::read_to_string(&manifest_path).unwrap();
    let content = format!("{}\n[run]\njvm-args = [\"-Xmx256m\"]\n", content);
    std::fs::write(&manifest_path, content).unwrap();

    // Run the project (jvm-args should be accepted without error)
    let output = Command::new(jargo_bin())
        .arg("run")
        .current_dir(&project_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "jargo run with jvm-args failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Hello, World!"));
}

#[test]
fn test_manifest_not_found_error() {
    let temp = TempDir::new().unwrap();

    // Try to build in empty directory
    let output = Command::new(jargo_bin())
        .arg("build")
        .current_dir(temp.path())
        .output()
        .unwrap();

    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Jargo.toml not found"));
}
