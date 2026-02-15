# Jargo

A Cargo-inspired build tool for Java, written in Rust. Targets small-to-medium Java projects. Not a Maven/Gradle replacement. See `docs/PRD.md` for full product requirements and `DESIGN.md` for condensed design decisions.

## Architecture

Rust CLI that orchestrates Java builds by shelling out to system `javac`/`java`. Core subsystems:

- **CLI** (clap): `new`, `init`, `build`, `run`, `test`, `check`, `clean`, `add`, `update`, `tree`, `fmt`, `fix`, `doc`
- **Manifest parser**: Reads `Jargo.toml` (TOML) and `Jargo.lock` (TOML)
- **Dependency resolver**: Fetches POMs/JARs from Maven Central, builds dependency graph, resolves conflicts with highest-version-wins
- **Compiler orchestrator**: Stages sources via symlink, assembles classpath, invokes `javac` via argument file, rewrites error paths
- **Test runner**: Invokes JUnit Platform with bundled harness, parses results, renders Cargo-style output
- **Formatter**: Bundles a Java formatter JAR, invokes via `java -jar`

## Critical Design Decisions

These are non-negotiable and affect multiple subsystems. Read `DESIGN.md` for rationale.

- **Flat source layout**: `src/` is the source root. No `com/example/app/` nesting. `base-package` in `Jargo.toml` defines the root Java package.
- **Single directory symlink**: `target/src-root/<base-package-as-path>` symlinks to `../../src` (or deeper). This is how `javac` sees the correct package structure. Never use per-file copies/symlinks.
- **Four classpaths**: compile, runtime, test-compile, test-runtime. Dependencies have `scope` (compile|runtime) and `expose` (bool, lib projects only). Follow Maven's scope mediation table for transitives.
- **Project types**: `type = "app"` (default) or `type = "lib"`. Affects `jargo run` availability, JAR manifest, `base-package` defaults, and `expose` semantics.
- **Implicit JUnit**: JUnit 5 is auto-included on test classpath. Not listed in `Jargo.toml` unless overriding version. Treat it as a built-in capability.
- **Error path rewriting**: `javac` errors reference staged paths (`target/src-root/myapp/Main.java`). Always rewrite to source paths (`src/Main.java`) in user-facing output.
- **`--release` not `--source`/`--target`**: The `java` field in manifest translates to `javac --release`.

## Build & Test

```bash
cargo build
cargo test
cargo run -- new test-project        # test project scaffolding
cargo run -- build                   # test build (from inside a Jargo project)
```

## Code Style

- `cargo fmt` before committing
- Use `thiserror` for error types, `anyhow` for application-level errors
- Prefer explicit error handling; no `.unwrap()` outside of tests
- Modules map to subsystems: `cli`, `manifest`, `resolver`, `compiler`, `test_runner`, `formatter`, `cache`
- Integration tests in `tests/` that create real Jargo projects, invoke commands, and assert outputs

## Key File Formats

**Jargo.toml** (user-authored):
```toml
[package]
name = "my-app"
version = "0.1.0"
type = "app"
java = "21"
base-package = "myapp"     # optional for app, defaults to name

[dependencies]
"com.google.guava:guava" = "33.0.0-jre"
"org.postgresql:postgresql" = { version = "42.7.1", scope = "runtime" }

[dev-dependencies]
"org.assertj:assertj-core" = "3.25.1"

[run]
jvm-args = ["-Xmx512m"]

[format]
indent = 2    # default 4
```

**Jargo.lock** (generated):
```toml
[[dependency]]
group = "com.google.guava"
artifact = "guava"
version = "33.0.0-jre"
sha256 = "abcdef..."
```

## Maven Central

- JAR URL pattern: `https://repo1.maven.org/maven2/{group-with-slashes}/{artifact}/{version}/{artifact}-{version}.jar`
- POM at same path with `.pom` extension
- Check for `.module` file first (Gradle metadata, JSON, has richer dependency info), fall back to POM (XML)
- Search API: `https://search.maven.org/solrsearch/select`
- Local cache: `~/.jargo/cache/` mirrors Maven Central directory structure

## When Compacting

Always preserve: the list of modified files, current implementation status of each CLI command, any failing tests and their error messages, and the active chunk/task scope from DESIGN.md.