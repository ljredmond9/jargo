use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

use crate::errors::JargoError;

/// Whether a fetched metadata file is a Gradle `.module` (JSON) or Maven `.pom` (XML).
#[derive(Debug, Clone, PartialEq)]
pub enum MetadataFormat {
    Module,
    Pom,
}

/// A cached metadata file and its format.
pub struct FetchedMetadata {
    pub path: PathBuf,
    pub format: MetadataFormat,
}

/// Fetch metadata for an artifact, preferring `.module` over `.pom`.
///
/// Returns the cached file if already present; downloads otherwise.
/// Tries `.module` first; falls back to `.pom` if `.module` is not available.
pub fn fetch_metadata(group: &str, artifact: &str, version: &str) -> Result<FetchedMetadata> {
    let dir = artifact_dir(group, artifact, version)?;
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create cache dir {}", dir.display()))?;

    // Check for cached .module
    let module_path = dir.join(artifact_filename(artifact, version, "module"));
    if module_path.exists() {
        return Ok(FetchedMetadata { path: module_path, format: MetadataFormat::Module });
    }

    // Check for cached .pom
    let pom_path = dir.join(artifact_filename(artifact, version, "pom"));
    if pom_path.exists() {
        return Ok(FetchedMetadata { path: pom_path, format: MetadataFormat::Pom });
    }

    // Not cached — fetch from Maven Central
    let client = http_client()?;

    // Try .module first
    let module_url = maven_central_url(group, artifact, version, "module");
    if try_download(&client, &module_url, &module_path)? {
        println!("  Fetching  {}:{}:{} (.module)", group, artifact, version);
        return Ok(FetchedMetadata { path: module_path, format: MetadataFormat::Module });
    }

    // Fall back to .pom
    let pom_url = maven_central_url(group, artifact, version, "pom");
    println!("  Fetching  {}:{}:{}", group, artifact, version);
    if try_download(&client, &pom_url, &pom_path)? {
        return Ok(FetchedMetadata { path: pom_path, format: MetadataFormat::Pom });
    }

    Err(JargoError::DependencyNotFound(
        group.to_string(),
        artifact.to_string(),
        version.to_string(),
    )
    .into())
}

/// Fetch the JAR for an artifact.
///
/// Returns `(path_to_jar, sha256_hex)`. The sha256 is read from a companion
/// `.jar.sha256` file if the JAR is already cached, or computed and stored
/// after a fresh download.
pub fn fetch_jar(group: &str, artifact: &str, version: &str) -> Result<(PathBuf, String)> {
    let dir = artifact_dir(group, artifact, version)?;
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create cache dir {}", dir.display()))?;

    let jar_path = dir.join(artifact_filename(artifact, version, "jar"));
    let sha_path = dir.join(artifact_filename(artifact, version, "jar.sha256"));

    if jar_path.exists() && sha_path.exists() {
        let sha256 = fs::read_to_string(&sha_path)
            .with_context(|| format!("failed to read {}", sha_path.display()))?
            .trim()
            .to_string();
        return Ok((jar_path, sha256));
    }

    // Download the JAR
    let url = maven_central_url(group, artifact, version, "jar");
    println!("  Fetching  {}:{}:{} (jar)", group, artifact, version);

    let client = http_client()?;
    if !try_download(&client, &url, &jar_path)? {
        return Err(JargoError::DependencyNotFound(
            group.to_string(),
            artifact.to_string(),
            version.to_string(),
        )
        .into());
    }

    let sha256 = compute_sha256(&jar_path)?;
    fs::write(&sha_path, &sha256)
        .with_context(|| format!("failed to write {}", sha_path.display()))?;

    Ok((jar_path, sha256))
}

/// Return the cache directory for a specific artifact version.
///
/// Structure mirrors Maven Central: `~/.jargo/cache/{group-path}/{artifact}/{version}/`
pub fn artifact_dir(group: &str, artifact: &str, version: &str) -> Result<PathBuf> {
    Ok(cache_base()?.join(group_to_path(group)).join(artifact).join(version))
}

// --- Pure helpers (pub for unit testing) ---

/// Convert a Maven group ID to a directory path segment.
///
/// `"com.google.guava"` → `"com/google/guava"`
pub fn group_to_path(group: &str) -> String {
    group.replace('.', "/")
}

/// Build the full Maven Central URL for a given artifact and file extension.
pub fn maven_central_url(group: &str, artifact: &str, version: &str, ext: &str) -> String {
    format!(
        "https://repo1.maven.org/maven2/{}/{}/{}/{}",
        group_to_path(group),
        artifact,
        version,
        artifact_filename(artifact, version, ext),
    )
}

/// Build the standard Maven filename for an artifact.
///
/// `("guava", "33.0.0-jre", "jar")` → `"guava-33.0.0-jre.jar"`
pub fn artifact_filename(artifact: &str, version: &str, ext: &str) -> String {
    format!("{}-{}.{}", artifact, version, ext)
}

// --- Private helpers ---

fn cache_base() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .context("could not determine home directory ($HOME is not set)")?;
    Ok(PathBuf::from(home).join(".jargo").join("cache"))
}

fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("failed to create HTTP client")
}

/// Download `url` to `dest`, writing atomically via a `.tmp` sibling file.
///
/// Returns `Ok(true)` on success, `Ok(false)` if the server returned 404,
/// and `Err` on any other failure.
fn try_download(client: &reqwest::blocking::Client, url: &str, dest: &Path) -> Result<bool> {
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("HTTP request failed: {}", url))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(false);
    }

    if !response.status().is_success() {
        bail!("HTTP {} fetching {}", response.status(), url);
    }

    let bytes = response
        .bytes()
        .with_context(|| format!("failed to read response body from {}", url))?;

    // Atomic write: write to .tmp first, then rename
    let tmp = dest.with_extension("tmp");
    fs::write(&tmp, &bytes)
        .with_context(|| format!("failed to write temporary file {}", tmp.display()))?;
    fs::rename(&tmp, dest)
        .with_context(|| format!("failed to rename {} to {}", tmp.display(), dest.display()))?;

    Ok(true)
}

/// Compute the SHA-256 digest of a file and return it as a lowercase hex string.
fn compute_sha256(path: &Path) -> Result<String> {
    let bytes =
        fs::read(path).with_context(|| format!("failed to read {} for sha256", path.display()))?;
    let hash = Sha256::digest(&bytes);
    Ok(hash.iter().map(|b| format!("{:02x}", b)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_group_to_path() {
        assert_eq!(group_to_path("com.google.guava"), "com/google/guava");
        assert_eq!(group_to_path("org.apache.commons"), "org/apache/commons");
        assert_eq!(group_to_path("junit"), "junit");
    }

    #[test]
    fn test_artifact_filename() {
        assert_eq!(artifact_filename("guava", "33.0.0-jre", "jar"), "guava-33.0.0-jre.jar");
        assert_eq!(artifact_filename("guava", "33.0.0-jre", "pom"), "guava-33.0.0-jre.pom");
        assert_eq!(
            artifact_filename("commons-lang3", "3.14.0", "jar"),
            "commons-lang3-3.14.0.jar"
        );
    }

    #[test]
    fn test_maven_central_url() {
        assert_eq!(
            maven_central_url("com.google.guava", "guava", "33.0.0-jre", "jar"),
            "https://repo1.maven.org/maven2/com/google/guava/guava/33.0.0-jre/guava-33.0.0-jre.jar"
        );
        assert_eq!(
            maven_central_url("org.apache.commons", "commons-lang3", "3.14.0", "pom"),
            "https://repo1.maven.org/maven2/org/apache/commons/commons-lang3/3.14.0/commons-lang3-3.14.0.pom"
        );
    }

    #[test]
    fn test_compute_sha256_known_value() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.txt");
        // SHA-256 of empty string is well-known
        fs::write(&file, b"").unwrap();
        let hash = compute_sha256(&file).unwrap();
        assert_eq!(hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn test_compute_sha256_known_content() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, b"hello world").unwrap();
        let hash = compute_sha256(&file).unwrap();
        // SHA-256("hello world") — verified against sha2 crate output
        assert_eq!(hash, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
        // Also verify the output format: 64 lowercase hex chars
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_artifact_dir_structure() {
        // Just verify the path is structured correctly relative to cache base.
        // We can't check the absolute path without knowing $HOME, but we can
        // verify the suffix.
        let dir = artifact_dir("com.google.guava", "guava", "33.0.0-jre").unwrap();
        let dir_str = dir.to_string_lossy();
        assert!(dir_str.contains(".jargo/cache/com/google/guava/guava/33.0.0-jre"));
    }
}
