* Chunk 1: Project scaffolding and CLI skeleton. Initialize a Rust project with a CLI framework (clap is the standard choice). Implement jargo new and jargo init — they create the directory structure, generate Jargo.toml, and scaffold Main.java and MainTest.java with correct package declarations. At the end of this chunk, you can run jargo new my-app and get a valid project structure. This is satisfying because it's visible and tangible immediately.

* Chunk 2: The compilation pipeline. Implement jargo build and jargo clean for a project with zero dependencies. This means TOML parsing for Jargo.toml, the symlink staging step, invoking javac with argument files, the error path rewriting, and JAR assembly. At the end of this chunk, jargo new my-app && cd my-app && jargo build produces a JAR. This is the first "it actually works" moment.

* Chunk 3: jargo run. Wire up jargo run to compile (if needed) and invoke java with the right classpath and main class. Add the -- argument separator for passing args to the Java program. Add the Cargo-style status output. At the end of this chunk, you have the full jargo new && jargo run workflow end to end.

* Chunk 4: Dependency resolution, phase 1. This is the biggest chunk and you might want to break it further. The sub-pieces are: HTTP fetching from Maven Central, POM XML parsing (basic, no parent POMs yet), transitive dependency graph construction, version conflict resolution (highest wins), lock file generation and reading, and local cache management. At the end of this chunk, you can add a simple dependency like commons-lang3 to Jargo.toml and have it resolve, download, and appear on the compile classpath.

* Chunk 5: Dependency resolution, phase 2. Parent POM resolution, property substitution, dependencyManagement sections. This unlocks the majority of real-world libraries. Test it with something like Guava or Jackson that uses parent POMs.

* Chunk 6: Testing. Implement jargo test — include JUnit JARs on the test classpath automatically, compile test sources, invoke the JUnit Platform with the custom test harness, parse and render results in Cargo style.

* Chunk 7: jargo check, jargo fix, jargo add. These are all relatively small features that round out the daily workflow. check is a thin variant of build, fix walks source files and corrects package declarations, add queries the Maven Central search API.

* Chunk 8: jargo tree, jargo update, jargo fmt. Polish features that improve the experience but aren't required for the core workflow.