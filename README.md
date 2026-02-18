# Jargo

A Cargo-inspired build tool for Java, written in Rust.

Jargo brings the ergonomics of `cargo` to Java projects: a single manifest file, flat source layout, automatic dependency fetching from Maven Central, and sensible defaults — without the complexity of Maven or Gradle.

> **Status:** Early development. Core commands (`new`, `init`, `build`, `clean`) are implemented. Dependency resolution, testing, and remaining commands are in progress.

## Installation

Build from source (requires Rust toolchain):

```bash
git clone https://github.com/ljredmond9/jargo
cd jargo
cargo build --release
# Binary at ./target/release/jargo
```

## Quick Start

```bash
jargo new my-app
cd my-app
jargo build
```

This creates a project, compiles it, and produces `target/my-app.jar`.

## Project Structure

```
my-app/
├── Jargo.toml       # project manifest
├── Jargo.lock       # generated lock file
├── src/             # Java source files (flat — no package directory nesting)
├── test/            # test sources
├── resources/       # bundled into JAR at build time
└── target/          # build output (deleted by jargo clean)
```

Source files live flat in `src/` regardless of package. The `base-package` field in `Jargo.toml` defines the root Java package; Jargo handles the package-to-directory mapping internally during compilation.

## Manifest: Jargo.toml

```toml
[package]
name = "my-app"
version = "0.1.0"
type = "app"          # "app" (default) or "lib"
java = "21"
base-package = "myapp"

[dependencies]
"com.google.guava:guava" = "33.0.0-jre"
"org.postgresql:postgresql" = { version = "42.7.1", scope = "runtime" }

[dev-dependencies]
"org.assertj:assertj-core" = "3.25.1"

[run]
jvm-args = ["-Xmx512m"]

[format]
indent = 4
```

Dependencies use Maven coordinates (`groupId:artifactId = "version"`). JUnit 5 is included automatically on the test classpath — no need to declare it.

## Commands

| Command | Description | Status |
|---------|-------------|--------|
| `jargo new <name>` | Create a new project in a new directory | Implemented |
| `jargo init` | Initialize a project in the current directory | Implemented |
| `jargo build` | Compile and assemble a JAR | Implemented |
| `jargo clean` | Delete the `target/` directory | Implemented |
| `jargo run [-- <args>]` | Compile and run (app projects only) | Planned |
| `jargo test` | Compile and run JUnit tests | Planned |
| `jargo check [--fmt]` | Check for errors without producing a JAR | Planned |
| `jargo add <group:artifact>` | Add a dependency | Planned |
| `jargo update` | Re-resolve and regenerate the lock file | Planned |
| `jargo tree` | Print the dependency graph | Planned |
| `jargo fmt` | Format source files | Planned |
| `jargo fix` | Auto-correct package declarations | Planned |
| `jargo doc` | Generate Javadoc | Planned |

Flags for `new`/`init`: `--lib` creates a library project instead of an application.

## Development

```bash
cargo build          # build
cargo test           # run tests
cargo fmt            # format (required before committing)

cargo run -- new test-project   # test scaffolding
cargo run -- build              # test build (run from inside a Jargo project)
```

Integration tests live in `tests/` and create real Jargo projects to exercise commands end to end.