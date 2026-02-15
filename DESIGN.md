# Jargo Design Decisions

Condensed reference for implementation. See `docs/PRD.md` for full rationale (if needed. should not be needed. the PRD is large.)

## Manifest: Jargo.toml

### [package]
| Field | Required | Default | Notes |
|-------|----------|---------|-------|
| name | yes | — | Used for JAR name and default base-package |
| version | yes | — | Semver |
| type | no | `"app"` | `"app"` or `"lib"` |
| java | yes | — | Translates to `javac --release` |
| base-package | no | project name (app) | Strongly encouraged for lib |
| main-class | no | `"Main"` | App only. Relative to base-package |

### [dependencies] and [dev-dependencies]
- Maven coordinates: `"groupId:artifactId" = "version"`
- Expanded form: `{ version = "x", scope = "runtime", expose = true }`
- `scope`: `"compile"` (default) or `"runtime"`
- `expose`: `false` (default). Lib projects only. When true, consumers get this on compile classpath
- Exact versions only (no ranges yet)
- JUnit 5 is implicit. Listing it in dev-dependencies overrides the built-in version

### [run]
- `jvm-args`: array of strings passed to `java`

### [format]
- `indent`: integer, default 4. The only configurable formatting option

## Directory Layout

```
project/
├── Jargo.toml
├── Jargo.lock          # generated
├── src/                # main sources, flat (no package-mirroring dirs)
├── test/               # test sources, same mapping as src/
├── resources/          # bundled into JAR at build time
├── test-resources/     # available during test execution only
└── target/             # build output, deleted by jargo clean
    ├── src-root/       # staging symlink
    ├── classes/        # compiled .class files
    ├── test-classes/   # compiled test .class files
    └── {name}.jar      # final artifact
```

### Package mapping
- `src/Foo.java` → `package {base-package};`
- `src/util/Bar.java` → `package {base-package}.util;`
- Source files MUST contain correct `package` declaration (not injected by Jargo)
- `jargo new`/`init` generates files with correct declarations
- `jargo check` verifies, `jargo fix` auto-corrects

## Compilation

### Staging (single directory symlink)
1. Create `target/src-root/` directory
2. Create symlink: `target/src-root/{base-package-as-path}` → relative path to `src/`
   - `base-package = "myapp"` → `target/src-root/myapp` → `../../src`
   - `base-package = "com.example.myapp"` → `target/src-root/com/example/myapp` → `../../../../src`
3. Invoke `javac -sourcepath target/src-root ...`
4. Windows fallback: per-file symlinks → file copies

### javac invocation
- Write args to `target/javac-args.txt`, invoke `javac @target/javac-args.txt`
- Use `--release {java}` (not `--source`/`--target`)
- Pass compile classpath via `-classpath`
- Output to `target/classes/` via `-d`

### Error path rewriting
- Post-process javac stderr
- Replace `target/src-root/{base-package-path}/` with `src/`
- Apply to both errors and warnings

### JAR assembly
- Package `target/classes/` + `resources/` into `target/{name}.jar`
- App: include `Main-Class` in `META-INF/MANIFEST.MF`
- Lib: no `Main-Class`
- `--uber` flag: unpack all dependency JARs into the JAR

## Dependency Resolution

### Fetching
- Maven Central URL: `repo1.maven.org/maven2/{group-path}/{artifact}/{version}/{artifact}-{version}.{ext}`
- Check for `.module` first (JSON, Gradle metadata), fall back to `.pom` (XML)
- Cache at `~/.jargo/cache/{group-path}/{artifact}/{version}/`

### Resolution algorithm
- Breadth-first traversal from direct dependencies
- For each dependency: fetch metadata, extract transitive deps, add to queue
- Conflict resolution: highest version wins (not nearest wins)
- Track `(groupId, artifactId) → resolved version` in hashmap
- If new version is higher, update map and re-process that artifact's deps
- Cycle detection required (circular deps exist in Maven Central)
- Skip optional dependencies

### Phased POM support
- **Phase 1**: Direct deps, transitive via POMs, scope filtering, skip optional
- **Phase 2**: Parent POM chains, `${property}` substitution, `<dependencyManagement>`
- **Phase 3**: Exclusions, BOMs (`scope=import`), version ranges in upstream POMs

### Version comparison
- Dot-separated numeric segments compared numerically
- `-SNAPSHOT` < release version
- Follow Maven's ordering for common qualifiers

## Classpaths

### Four classpaths
| Classpath | Contents |
|-----------|----------|
| Compile | compile-scope deps |
| Runtime | compile + runtime-scope deps |
| Test compile | compile classpath + dev-deps + JUnit |
| Test runtime | runtime classpath + dev-deps + JUnit |

### Scope mediation (transitive)
| Direct → Lib | Lib → Transitive | Effective |
|--------------|-------------------|-----------|
| compile | compile | compile |
| compile | runtime | runtime |
| compile | provided | omitted |
| runtime | compile | runtime |
| runtime | runtime | runtime |
| test | compile | test |
| test | runtime | test |

### Library exposure
- `expose = true`: dep goes on consumer's compile + runtime classpath
- `expose = false` (default): dep goes on consumer's runtime classpath only
- Only meaningful for lib projects
- Gradle `.module` files provide this info for third-party libs

## Lock File: Jargo.lock

- TOML format, `[[dependency]]` array
- Fields: `group`, `artifact`, `version`, `sha256`
- `jargo build`: use if present, generate if absent
- `jargo update`: re-resolve and regenerate
- `jargo add`: add dep, re-resolve, update

## Testing

- JUnit 5 auto-included (Platform + Jupiter JARs) when test files exist
- Bundled Java test harness implements `TestExecutionListener`
- Harness outputs structured results → Jargo parses and renders Cargo-style
- Test compilation: test classpath = compile classpath + dev-deps + JUnit
- Test execution: test runtime classpath

## Formatting

- Bundled Java formatter JAR (embedded in Jargo binary via `include_bytes!`)
- Extracted to `~/.jargo/tools/` on first use
- Invoked via `java -jar` with all `.java` files in single invocation
- Only config: `indent` (default 4) in `[format]` section
- `jargo fmt`: reformat in place
- `jargo check --fmt`: verify without modifying (CI-friendly)
- `jargo build` does NOT enforce formatting

## Commands (implementation order)

1. `new`/`init` — scaffold project
2. `build` — compile + JAR
3. `run` — compile + execute (app only)
4. `clean` — delete target/
5. `check` — compile without JAR, verify packages, optional `--fmt`
6. `test` — compile + run JUnit
7. `add` — query Maven Central search API, update manifest + lock
8. `tree` — print dependency graph
9. `update` — re-resolve lock file
10. `fmt` — run formatter
11. `fix` — correct package declarations
12. `doc` — invoke javadoc

## jargo run flow

1. Resolve/download missing dependencies
2. Create staging symlink if needed
3. Invoke `javac` (skip if target/classes/ up to date — stretch goal)
4. Invoke `java` with runtime classpath + main class
5. Print: `Compiling {name} v{version} (java {java})` then `Running {name}`
6. Stream app stdout/stderr directly to terminal
7. `--` separates Jargo args from app args