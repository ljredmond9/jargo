use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "jargo", about = "A Cargo-inspired build tool for Java")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Create a new Jargo project
    New {
        /// Project name
        name: String,
        /// Create a library project instead of an application
        #[arg(long)]
        lib: bool,
    },
    /// Initialize a Jargo project in the current directory
    Init {
        /// Create a library project instead of an application
        #[arg(long)]
        lib: bool,
    },
    /// Compile the project and assemble a JAR
    Build,
    /// Compile and run the project (app only)
    Run {
        /// Arguments to pass to the Java program
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Run tests
    Test,
    /// Check the project for errors without producing a JAR
    Check {
        /// Also check formatting
        #[arg(long)]
        fmt: bool,
    },
    /// Remove the target directory
    Clean,
    /// Add a dependency
    Add {
        /// Maven coordinate (groupId:artifactId)
        coordinate: String,
        /// Specific version (otherwise queries Maven Central for latest)
        #[arg(long)]
        version: Option<String>,
    },
    /// Update dependencies to latest versions and regenerate lock file
    Update,
    /// Display the dependency tree
    Tree,
    /// Format source files
    Fmt,
    /// Auto-fix package declarations
    Fix,
    /// Generate Javadoc
    Doc,
}
