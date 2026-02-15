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

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
