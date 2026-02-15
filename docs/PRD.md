# Jargo: A Cargo-Inspired Build Tool for Java

**Product Requirements Document — February 2026**

---

## Table of Contents

1. [Overview](#1-overview)
2. [Project Manifest: Jargo.toml](#2-project-manifest-jargotoml)
3. [Directory Layout](#3-directory-layout)
4. [Compilation Pipeline](#4-compilation-pipeline)
5. [Dependency Resolution](#5-dependency-resolution)
6. [Lock File: Jargo.lock](#6-lock-file-jargolock)
7. [Local Cache](#7-local-cache)
8. [Command Surface](#8-command-surface)
9. [Testing](#9-testing)
10. [Formatting](#10-formatting)
11. [Scope and Non-Goals](#11-scope-and-non-goals)
12. [Technical Implementation](#12-technical-implementation)

---

## 1. Overview

### 1.1 Purpose

Jargo is a build tool for Java, inspired by Rust's Cargo and built with Rust. It aims to bring the simplicity, ergonomics, and opinionated conventions of Cargo to Java development. Jargo targets small-to-medium sized Java projects and is not intended to be a full replacement for Maven or Gradle.

The primary purpose of Jargo is educational — an exercise in building a real-world Rust application that solves a genuine problem. If the resulting tool proves useful for simple Java projects, that is an additional benefit.

### 1.2 Design Philosophy

- **Convention over configuration.** Jargo should work with minimal setup. A new project should compile, run, and test with zero configuration beyond a project name.
- **Opinionated defaults.** Where Java's ecosystem offers many choices (directory layout, test framework, formatting), Jargo picks one and makes it the default.
- **Honest about Java.** Jargo borrows Cargo's UX philosophy but does not pretend Java is Rust. Where Java's ecosystem requires complexity (Maven Central coordinates, package declarations, classpaths), Jargo manages that complexity rather than hiding it.
- **Deviate where the value is clear.** Jargo is willing to break from Java convention when doing so produces a meaningfully better developer experience, but not for novelty's sake.

### 1.3 Target Audience

Developers working on small-to-medium Java projects who want a lightweight, fast, convention-driven build tool. This includes developers coming from Rust who want a familiar workflow for Java, developers building small utilities, CLI tools, or libraries who find Maven and Gradle heavyweight, and learners who want to start a Java project without wrestling with XML or Groovy build scripts.

---

## 2. Project Manifest: Jargo.toml

### 2.1 Format

The project manifest is a TOML file named **Jargo.toml** located at the project root. It serves the same role as Cargo.toml in Rust projects or pom.xml in Maven projects, but with minimal verbosity.

### 2.2 The [package] Section

The `[package]` section contains project metadata and configuration:

```toml
[package]
name = "my-app"
version = "0.1.0"
type = "app"              # "app" or "lib"
java = "21"               # minimum Java version
base-package = "com.example.myapp"  # optional for app, encouraged for lib
main-class = "Main"       # only meaningful for type = "app"
```

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | The project name. Used for JAR naming and default base-package. |
| `version` | Yes | The project version, following semver conventions. |
| `type` | Yes | Either `"app"` (runnable application) or `"lib"` (library). Defaults to `"app"`. |
| `java` | Yes | The minimum Java version. Translated to `javac --release` flag. |
| `base-package` | No | The root Java package for all source files. Defaults to the project name for app projects. Strongly encouraged for lib projects. |
| `main-class` | No | The entry point class name (relative to base-package). Only meaningful for app projects. Defaults to `"Main"`. |

### 2.3 Project Types: App vs Lib

Jargo makes an explicit distinction between application projects (`"app"`) and library projects (`"lib"`), inspired by Cargo's binary/library crate distinction. The term "app" is used instead of Cargo's "bin" because Java does not produce native binaries — it produces bytecode that runs on the JVM.

| Aspect | App | Lib |
|--------|-----|-----|
| Purpose | Runnable application | Reusable library for other projects |
| Entry point | Requires a class with a `main` method | No `main` method expected |
| `base-package` | Optional, defaults to project name | Strongly encouraged (collision avoidance) |
| `jargo run` | Supported | Error: "this is a library project" |
| `jargo build` output | JAR with `Main-Class` manifest entry | JAR without `Main-Class` entry |
| Scaffolded file | `src/Main.java` | `src/Lib.java` |

### 2.4 Dependencies

Dependencies are declared in two sections: `[dependencies]` for compile/runtime dependencies and `[dev-dependencies]` for test-only dependencies. Dependencies use Maven Central coordinates in `groupId:artifactId` format.

The simple form uses a plain version string:

```toml
[dependencies]
"com.google.guava:guava" = "33.0.0-jre"
"org.apache.commons:commons-lang3" = "3.14.0"

[dev-dependencies]
# JUnit 5 is implicit and does not need to be listed.
# Listing it here overrides the built-in version.
"org.assertj:assertj-core" = "3.25.1"
```

The expanded form allows specifying scope and API exposure:

```toml
[dependencies]
"org.postgresql:postgresql" = { version = "42.7.1", scope = "runtime" }
"com.google.guava:guava" = { version = "33.0.0-jre", expose = true }
```

| Field | Default | Description |
|-------|---------|-------------|
| `version` | (required) | The exact version of the dependency. |
| `scope` | `"compile"` | Dependency scope: `"compile"` or `"runtime"`. Determines classpath placement. |
| `expose` | `false` | Whether this dependency is part of the library's public API. Only meaningful for lib projects. When `true`, consumers get this dependency on their compile classpath. |

**Version specification:** Jargo requires exact versions in Jargo.toml. Version ranges are a planned future feature.

### 2.5 JVM Arguments

Default JVM arguments for `jargo run` can be specified in the manifest:

```toml
[run]
jvm-args = ["-Xmx512m", "-Dfoo=bar"]
```

### 2.6 Formatter Configuration

```toml
[format]
indent = 2    # default is 4
```

See [Section 10: Formatting](#10-formatting) for details.

---

## 3. Directory Layout

### 3.1 Flat Source Layout

Jargo uses a flat, Cargo-inspired directory layout that eliminates the deep package-mirroring directory nesting traditional in Java projects. The `base-package` value in Jargo.toml defines the root Java package, and the directory structure under `src/` maps to sub-packages relative to that root.

```
my-project/
├── Jargo.toml
├── Jargo.lock
├── src/
│   ├── Main.java              # package myapp;
│   └── util/
│       └── StringHelper.java  # package myapp.util;
├── test/
│   └── MainTest.java          # package myapp;
├── resources/
│   └── config.properties
└── test-resources/
    └── test-config.properties
```

| Directory | Purpose |
|-----------|---------|
| `src/` | Main source files. Subdirectories map to sub-packages. |
| `test/` | Test source files. Same sub-package mapping as `src/`. |
| `resources/` | Non-code files bundled into the JAR (config files, templates, etc.). |
| `test-resources/` | Non-code files available only during test execution. |
| `target/` | Build output directory. Created by Jargo, deleted by `jargo clean`. |

### 3.2 Package Declaration Management

Java source files must contain a `package` declaration that matches their location in the directory structure relative to `base-package`. Jargo manages and verifies these declarations but does not inject them silently — source files remain valid Java that any tool or IDE can understand.

- **`jargo new` / `jargo init`** generates source files with the correct package declaration already present.
- **`jargo check`** verifies that every source file's package declaration matches its location and reports mismatches.
- **`jargo fix`** automatically corrects incorrect or missing package declarations.

This approach gives developers a flat, clean source tree while maintaining full compatibility with IDEs, static analysis tools, formatters, debuggers, and the broader Java tooling ecosystem.

### 3.3 Build Output Layout

```
target/
├── src-root/          # staging symlink for javac
├── classes/           # compiled .class files
├── test-classes/      # compiled test classes
└── my-app.jar         # the final packaged artifact
```

---

## 4. Compilation Pipeline

### 4.1 Compiler Invocation

Jargo shells out to **`javac`** (which must be available on the system PATH as part of a JDK installation). Jargo does not embed or bundle a Java compiler.

The `java` version field in Jargo.toml is translated to the **`--release`** flag (not `--source`/`--target`), which constrains both language features and available standard library APIs to the specified version. This prevents accidental use of APIs unavailable on the target Java version.

### 4.2 Source Staging via Single Directory Symlink

Because Jargo's flat source layout does not match the directory structure that `javac` expects (where directories mirror package names), Jargo bridges this gap using a single directory symlink.

During a build, Jargo creates the directory **`target/src-root/`** and within it a symlink from the base-package path to the `src/` directory. For example, if `base-package` is `"myapp"`, Jargo creates:

```
target/src-root/myapp → ../../src
```

For a deeper base-package like `"com.example.myapp"`:

```
target/src-root/com/example/myapp → ../../../../src
```

Jargo then invokes `javac` with **`-sourcepath target/src-root`**. This approach has several advantages:

- **Minimal overhead.** One symlink, created once. No per-file copies or symlinks.
- **Always in sync.** Adding or removing files in `src/` is instantly reflected through the symlink.
- **Transparent to editors.** Error messages and debugger paths resolve through the symlink to the actual source files.

**Windows compatibility:** On Windows, where symlinks require elevated permissions or Developer Mode, Jargo falls back to per-file symlinks (using `mklink`) or file copies as a last resort.

### 4.3 Argument Files

To avoid command-line length limits (particularly on Windows), Jargo writes all compiler arguments to **`target/javac-args.txt`** and invokes `javac @target/javac-args.txt`. This file contains the classpath, source file list, and all compiler flags.

### 4.4 Resource Handling

During the build, Jargo copies the contents of **`resources/`** into **`target/classes/`** so that resource files are bundled into the JAR at the correct classpath locations. Similarly, **`test-resources/`** contents are made available during test execution.

### 4.5 Error Reporting

Because `javac` receives staged file paths (e.g., `target/src-root/myapp/Main.java`), its error messages reference those paths. Jargo post-processes `javac`'s stderr output to rewrite staged paths back to the original source locations (e.g., `src/Main.java`), ensuring error messages are meaningful to the developer.

### 4.6 JAR Packaging

After compilation, Jargo assembles a JAR from `target/classes/` with a generated `META-INF/MANIFEST.MF`. For app projects, the manifest includes a `Main-Class` entry derived from the `main-class` field in Jargo.toml.

- **Default:** Thin JAR containing only the project's compiled classes and resources.
- **Option:** Uber/fat JAR via **`jargo build --uber`**, which bundles all dependency classes into a single self-contained JAR.

The `jargo run` command handles classpath assembly transparently, so the thin JAR default does not affect the development workflow.

### 4.7 Incremental Compilation

Incremental compilation is a stretch goal. The initial implementation recompiles all source files on every build. For the small-to-medium projects Jargo targets, full recompilation is fast enough to be imperceptible.

---

## 5. Dependency Resolution

### 5.1 Maven Central Integration

Jargo resolves dependencies from Maven Central using its predictable URL structure. For a given artifact coordinate, the POM and JAR are fetched via HTTP GET from deterministic URLs. No authentication is required for reading.

For the `jargo add` command, Jargo queries the Maven Central search API (`search.maven.org`) to resolve the latest version of an artifact.

### 5.2 POM Parsing (Phased Approach)

Maven POM files describe an artifact's dependencies and metadata. Jargo implements POM parsing in three phases:

**Phase 1 — Minimum Viable Resolver:** Direct dependencies with explicit versions, transitive resolution by parsing POMs, scope filtering (include `compile` and `runtime`, exclude `test`/`provided`/`system`), skip optional dependencies.

**Phase 2 — Parent POMs and Properties:** Fetch and merge parent POM chains, resolve `${property}` variable substitutions, handle `<dependencyManagement>` sections for centralized version control.

**Phase 3 — Advanced Features:** Dependency exclusions, BOMs (Bill of Materials) with `scope=import`, version ranges in upstream POMs, and any additional Maven features encountered through real-world usage.

### 5.3 Gradle Module Metadata

Many libraries published to Maven Central by Gradle-based projects include a **`.module`** file (JSON format) alongside the POM. This file contains richer metadata than the POM, including the `api`/`implementation` dependency distinction.

When resolving a dependency, Jargo first checks for a `.module` file. If present, it is used as the primary metadata source, with the POM as fallback. This gives Jargo access to the expose/unexpose distinction for Gradle-published libraries without any additional effort from the library author.

### 5.4 Version Conflict Resolution

Jargo uses a **"highest version wins"** strategy for resolving version conflicts, where the highest version of a given artifact encountered anywhere in the dependency graph is selected. This differs from Maven's "nearest wins" strategy and is chosen because it produces fewer runtime `NoSuchMethodError` surprises — newer versions are generally backward compatible.

Version comparison follows Maven's version ordering algorithm: numeric comparison of dot-separated segments, with the understanding that `-SNAPSHOT` is lower than the release version.

### 5.5 Classpath Model

Jargo maintains four distinct classpaths:

| Classpath | Used By | Contains |
|-----------|---------|----------|
| Compile | `javac` (main sources) | compile-scope dependencies |
| Runtime | `java` (running the app) | compile-scope + runtime-scope dependencies |
| Test compile | `javac` (test sources) | All of compile classpath + dev-dependencies |
| Test runtime | `java` (running tests) | All of runtime classpath + dev-dependencies |

Dependencies declared with **`scope = "runtime"`** appear on the runtime classpath but not the compile classpath. This prevents accidental compile-time coupling to implementation details (e.g., a specific JDBC driver).

### 5.6 Transitive Scope Mediation

When resolving transitive dependencies, the effective scope is determined by combining the scope of the direct dependency with the scope declared in the transitive dependency's POM. Jargo follows Maven's scope mediation rules:

| Your Dep Scope | Transitive Scope | Effective Scope |
|-----------------|------------------|-----------------|
| compile | compile | compile |
| compile | runtime | runtime |
| compile | provided | (omitted) |
| runtime | compile | runtime |
| runtime | runtime | runtime |
| test | compile | test |
| test | runtime | test |

Provided and test dependencies of upstream libraries are always omitted from the transitive graph — they are only relevant for building that library, not for consuming it.

### 5.7 API Exposure for Libraries

For lib projects, dependencies marked with **`expose = true`** are placed on the compile classpath of any project that depends on the library. Unexposed dependencies (the default) are placed only on the runtime classpath of consumers. This prevents accidental coupling to transitive dependencies, following the same model as Gradle's `api` vs `implementation` configurations.

This distinction is only meaningful for lib projects. For app projects, the `expose` field is ignored.

### 5.8 Provided Scope

The `"provided"` scope (compile-only, not included at runtime) is deferred to a future version. It is primarily relevant for web applications and plugin architectures, which are outside Jargo's initial target use cases.

---

## 6. Lock File: Jargo.lock

Jargo.lock records the complete resolved dependency graph with pinned versions and checksums for reproducible builds. It is generated automatically and stored in TOML format.

```toml
[[dependency]]
group = "com.google.guava"
artifact = "guava"
version = "33.0.0-jre"
sha256 = "abcdef1234..."

[[dependency]]
group = "com.google.code.findbugs"
artifact = "jsr305"
version = "3.0.2"
sha256 = "fedcba4321..."
```

| Command | Lock File Behavior |
|---------|--------------------|
| `jargo build` | Uses lock file if present. Resolves and creates one if absent. |
| `jargo update` | Re-resolves all dependencies and regenerates the lock file. |
| `jargo add` | Adds the new dependency, re-resolves, and updates the lock file. |

---

## 7. Local Cache

Downloaded JARs and POMs are stored in a shared cache at **`~/.jargo/cache/`** with a directory structure mirroring Maven Central's layout:

```
~/.jargo/cache/
└── com/google/guava/guava/33.0.0-jre/
    ├── guava-33.0.0-jre.jar
    ├── guava-33.0.0-jre.pom
    └── guava-33.0.0-jre.jar.sha256
```

Dependencies are downloaded once and shared across all Jargo projects on the machine, analogous to Cargo's registry cache.

---

## 8. Command Surface

### 8.1 Core Commands

| Command | Description |
|---------|-------------|
| `jargo new <name>` | Create a new project in a new directory. Defaults to app type. Use `--lib` for library. |
| `jargo init` | Initialize a Jargo project in the current directory. |
| `jargo build` | Resolve dependencies, compile sources, produce a JAR. Use `--uber` for a fat JAR. |
| `jargo run [-- args]` | Compile and run the app. Everything after `--` is passed to the Java program. |
| `jargo test` | Compile and run tests with Cargo-style output formatting. |
| `jargo check` | Compile without producing a JAR. Optionally verify formatting with `--fmt`. |
| `jargo clean` | Delete the `target/` directory. |

### 8.2 Dependency Commands

| Command | Description |
|---------|-------------|
| `jargo add <coordinate>` | Query Maven Central for the latest version, add to Jargo.toml, update lock file. Use `--dev` for test dependencies. |
| `jargo update` | Re-resolve dependencies and regenerate Jargo.lock. |
| `jargo tree` | Print the resolved dependency graph in a readable tree format. |

### 8.3 Quality Commands

| Command | Description |
|---------|-------------|
| `jargo fmt` | Format all source files using an opinionated formatter. See [Section 10](#10-formatting). |
| `jargo fix` | Automatically correct missing or incorrect package declarations. |
| `jargo doc` | Generate Javadoc documentation to `target/doc/`. |

### 8.4 Implementation Priority

Commands are listed in recommended implementation order:

| Priority | Command | Rationale |
|----------|---------|-----------|
| 1 | `new` / `init` | Required to create projects. First thing any user runs. |
| 2 | `build` | The core of the tool. |
| 3 | `run` | The "wow, it just works" moment. |
| 4 | `clean` | Trivial to implement, immediately useful. |
| 5 | `check` | Small delta from build. |
| 6 | `test` | Important but requires JUnit Platform integration. |
| 7 | `add` | Big ergonomic win, requires Maven Central API integration. |
| 8 | `tree` | Falls out of the dependency resolver. |
| 9 | `update` | Natural companion to the lock file. |
| 10 | `fmt` | Nice to have, delegates to external formatter. |
| 11 | `fix` | Companion to check for package declaration management. |
| 12 | `doc` | Low priority, thin wrapper around javadoc. |

### 8.5 jargo run Details

When a developer types `jargo run`, the following sequence executes: resolve and download missing dependencies, stage source files, invoke `javac`, then invoke `java` with the correct classpath and main class. The application's stdout and stderr stream directly to the terminal.

Jargo prints Cargo-style status lines before the program's output:

```
  Compiling my-app v0.1.0 (java 21)
    Running my-app
```

The `--` separator passes arguments to the Java program: `jargo run -- --port 8080`. JVM arguments can be configured in the `[run]` section of Jargo.toml or via `--jvm-args` on the command line.

---

## 9. Testing

### 9.1 JUnit 5 as Built-In Test Framework

Jargo treats JUnit 5 (Jupiter) as a built-in capability rather than a user-managed dependency. JUnit is not listed in Jargo.toml — Jargo automatically includes the JUnit Platform and Jupiter API on the test classpath when test files are present.

This makes testing feel native to the tool, analogous to Rust's built-in `#[test]` attribute, while leveraging JUnit's mature ecosystem and universal IDE integration.

### 9.2 Version Management

Jargo ships with (or pins) a specific JUnit 5 version. If a developer needs a different version, they can explicitly add JUnit to `[dev-dependencies]`, which overrides the built-in version. The implicit JUnit version is documented and may be updated with new Jargo releases.

### 9.3 Test Discovery and Execution

Jargo invokes the JUnit Platform programmatically using a small bundled Java test harness. This harness implements a custom `TestExecutionListener` that reports results in a structured format that Jargo's Rust code parses and renders in Cargo's test output style:

```
running 3 tests
test adds_numbers ... ok
test greets_by_name ... ok
test handles_empty_input ... FAILED

failures:
    ---- handles_empty_input ----
    assertion failed: expected "Hello, !" but got "Hello, null!"

test result: FAILED. 2 passed; 1 failed; 0 ignored
```

### 9.4 Test Classpath

The test classpath includes: compiled test classes (`target/test-classes/`), compiled main classes (`target/classes/`), all compile and runtime dependencies, all dev-dependencies, `test-resources/`, and the JUnit Platform and Jupiter JARs.

### 9.5 Scaffolded Tests

`jargo new` generates a skeleton test file for app projects:

```java
package myapp;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class MainTest {
    @Test
    void it_works() {
        assertTrue(true);
    }
}
```

This ensures that `jargo test` works on a freshly scaffolded project with zero configuration.

---

## 10. Formatting

### 10.1 Philosophy

Jargo takes the position that code formatting is a solved problem. Following the precedent set by `gofmt` (Go) and `rustfmt` (Rust), Jargo provides an opinionated, mostly non-configurable formatter. There is one format for Jargo projects. This eliminates formatting debates and keeps pull requests free of whitespace noise.

### 10.2 The One Configuration Option: Indentation Width

Jargo allows exactly one formatting configuration: indentation width. This is specified in the `[format]` section of Jargo.toml:

```toml
[format]
indent = 2    # default is 4
```

The default is 4 spaces, which matches the most common Java convention. Developers who prefer 2-space indentation (as used by Google's Java style) can set this explicitly. No other formatting options are exposed — brace placement, import ordering, line wrapping, and all other stylistic decisions are fixed and non-configurable.

The rationale for this single exception is that indentation width is a scalar parameter that does not affect code structure. It is the most common reason developers reject an otherwise good formatter, and it touches readability and accessibility rather than pure aesthetics. All other formatting choices remain locked to prevent the configurability from escalating into a full style guide system.

### 10.3 Implementation

`jargo fmt` delegates to an external Java formatter (such as google-java-format or Palantir Java Format), which Jargo bundles as an embedded resource in the Jargo binary. The formatter JAR is extracted to `~/.jargo/tools/` on first use. The developer never installs or configures the formatter manually.

The formatter is invoked via `java -jar` with all `.java` files in `src/` and `test/` passed in a single JVM invocation to avoid per-file startup overhead.

### 10.4 Formatter Commands and CI Integration

- **`jargo fmt`** reformats all source files in place.
- **`jargo check --fmt`** verifies that all source files conform to the expected format without modifying them, suitable for CI pipelines.
- **`jargo build`** does not gatekeep on formatting — the formatter is a separate, opt-in step that is universally expected but not enforced at build time.

### 10.5 Output

```
$ jargo fmt
  Formatted 3 files

$ jargo fmt
  All 7 files already formatted

$ jargo check --fmt
  Checking formatting...
  src/util/Helper.java is not formatted

  1 file needs formatting. Run jargo fmt to fix.
```

### 10.6 Scope

`jargo fmt` handles code style only (indentation, brace placement, import ordering, line wrapping). Package declaration verification is handled separately by `jargo check` and `jargo fix`.

---

## 11. Scope and Non-Goals

### 11.1 In Scope (Future)

- **Version ranges** in Jargo.toml for flexible dependency specification.
- **Provided scope** for compile-only dependencies.
- **`jargo bench`** wrapping JMH for benchmarking.
- **Incremental compilation** to avoid recompiling unchanged files.

### 11.2 Explicitly Out of Scope

- **Multi-module projects** (Cargo workspaces equivalent).
- **Publishing to Maven Central** (requires GPG signing, Sonatype staging, and significant infrastructure).
- **Plugin system** for extending the build tool.
- **Annotation processing** (Lombok, MapStruct, Dagger).
- **Java Module System** (JPMS / `module-info.java`).
- **Resource filtering** or profile-based configuration.
- **Multi-release JARs** or native library bundling.

These features are either outside Jargo's target use case of small-to-medium projects, or represent complexity that would be better served by migrating to Gradle.

---

## 12. Technical Implementation

### 12.1 Language and Runtime

Jargo is implemented in **Rust**. It requires a JDK installation on the system PATH for compilation and execution of Java code. Jargo does not bundle or embed a JDK.

### 12.2 Key External Dependencies

- **`javac`** (from the system JDK) for compilation.
- **`java`** (from the system JDK) for running applications and tests.
- **Maven Central** (`repo1.maven.org`) for dependency resolution and downloading.
- **Maven Central Search API** (`search.maven.org`) for the `jargo add` command.

### 12.3 Error Reporting

Jargo aspires to Cargo-quality error messages. When dependency resolution fails or produces surprising results, Jargo provides clear explanations of which dependency chains led to conflicts and what versions each requires. Build errors include rewritten file paths pointing to original source files, not staged intermediates.