use thiserror::Error;

#[derive(Debug, Error)]
pub enum JargoError {
    #[error("destination `{0}` already exists")]
    ProjectExists(String),

    #[error("invalid project name `{0}`: {1}")]
    InvalidName(String, String),

    #[error("`Jargo.toml` already exists in current directory")]
    AlreadyInitialized,

    #[error("could not determine directory name")]
    NoDirName,

    #[error("Jargo.toml not found in current directory")]
    ManifestNotFound,

    #[error("failed to parse Jargo.toml: {0}")]
    ManifestParse(String),

    #[error("javac compilation failed")]
    CompilationFailed,

    #[error("javac not found in PATH")]
    JavacNotFound,

    #[error("java not found in PATH")]
    JavaNotFound,

    #[error("`jargo run` requires an app project (type = \"app\")")]
    NotAnApp,

    #[error("dependency `{0}:{1}` version `{2}` not found on Maven Central")]
    DependencyNotFound(String, String, String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
