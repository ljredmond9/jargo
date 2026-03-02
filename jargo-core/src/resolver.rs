use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::cache::{self, MetadataFormat};
use crate::context::GlobalContext;
use crate::gradle_module;
use crate::lockfile::{LockFile, LockedDependency};
use crate::manifest::{Dependency, JargoToml, Scope};
use crate::pom::{ParsedPom, TransitiveDep, TransitiveScope};
use crate::vprintln;

/// The output of dependency resolution: classpath JAR lists and lock file entries.
pub struct ResolvedDeps {
    /// JARs on the compile classpath (compile-scope deps only).
    pub compile_jars: Vec<PathBuf>,
    /// JARs on the runtime classpath (compile + runtime scope deps).
    pub runtime_jars: Vec<PathBuf>,
    /// Entries written to / read from Jargo.lock.
    pub lock_entries: Vec<LockedDependency>,
}

impl ResolvedDeps {
    fn empty() -> Self {
        Self {
            compile_jars: Vec::new(),
            runtime_jars: Vec::new(),
            lock_entries: Vec::new(),
        }
    }
}

/// Resolve dependencies for the project at `project_root`.
///
/// - If `Jargo.lock` exists: uses pinned versions from the lock file,
///   fetches any JARs not yet in the local cache, and builds classpaths.
/// - If `Jargo.lock` is absent: runs BFS resolution from Maven Central,
///   writes a new `Jargo.lock`, and returns the resulting classpaths.
///
/// Returns empty classpaths immediately when there are no dependencies.
pub fn resolve(
    gctx: &GlobalContext,
    project_root: &Path,
    manifest: &JargoToml,
) -> Result<ResolvedDeps> {
    let direct_deps = manifest.get_dependencies()?;

    if direct_deps.is_empty() {
        vprintln!(gctx, "  [verbose] no dependencies declared");
        return Ok(ResolvedDeps::empty());
    }

    let lock_path = project_root.join("Jargo.lock");

    if lock_path.exists() {
        let lock = LockFile::read(&lock_path)?;
        if lock_is_fresh(&direct_deps, &lock) {
            vprintln!(
                gctx,
                "  [verbose] lock file is up to date: {}",
                lock_path.display()
            );
            return resolve_from_lock(gctx, &lock);
        }
        vprintln!(gctx, "  [verbose] lock file is out of date, re-resolving");
    }

    println!("  Resolving dependencies...");
    let resolved = resolve_fresh(gctx, &direct_deps)?;

    let lock = LockFile {
        dependency: resolved.lock_entries.clone(),
    };
    vprintln!(gctx, "  [verbose] writing Jargo.lock");
    lock.write(&lock_path)
        .context("failed to write Jargo.lock")?;
    println!("     Locking dependencies");

    Ok(resolved)
}

/// Returns true when every direct dep in the manifest has an entry in the lock
/// file with the exact same version. If any dep is missing or has changed
/// version, the lock is considered stale and must be regenerated.
fn lock_is_fresh(direct_deps: &[Dependency], lock: &LockFile) -> bool {
    direct_deps.iter().all(|dep| {
        lock.dependency.iter().any(|entry| {
            entry.group == dep.group
                && entry.artifact == dep.artifact
                && entry.version == dep.version
        })
    })
}

// --- Lock-file path ---

/// Build classpaths from an existing `Jargo.lock` without re-resolving.
/// Fetches JARs from the local cache (downloading if absent).
fn resolve_from_lock(gctx: &GlobalContext, lock: &LockFile) -> Result<ResolvedDeps> {
    vprintln!(
        gctx,
        "  [verbose] lock file has {} entr{}",
        lock.dependency.len(),
        if lock.dependency.len() == 1 {
            "y"
        } else {
            "ies"
        }
    );

    let mut compile_jars = Vec::new();
    let mut runtime_jars = Vec::new();

    for entry in &lock.dependency {
        vprintln!(
            gctx,
            "  [verbose] fetching {}:{}:{} ({})",
            entry.group,
            entry.artifact,
            entry.version,
            entry.scope
        );
        let (jar_path, _sha256) =
            cache::fetch_jar(gctx, &entry.group, &entry.artifact, &entry.version).with_context(
                || {
                    format!(
                        "failed to fetch JAR for {}:{}:{}",
                        entry.group, entry.artifact, entry.version
                    )
                },
            )?;

        match entry.scope.as_str() {
            "compile" => {
                compile_jars.push(jar_path.clone());
                runtime_jars.push(jar_path);
            }
            _ => {
                // "runtime" or any unknown scope → runtime only
                runtime_jars.push(jar_path);
            }
        }
    }

    vprintln!(
        gctx,
        "  [verbose] classpath ready: {} compile JAR(s), {} runtime JAR(s)",
        compile_jars.len(),
        runtime_jars.len()
    );

    Ok(ResolvedDeps {
        compile_jars,
        runtime_jars,
        lock_entries: lock.dependency.clone(),
    })
}

// --- Fresh resolution ---

/// Resolve dependencies from Maven Central via BFS.
///
/// Algorithm:
/// 1. Seed the resolved map and work queue from `direct_deps`.
/// 2. For each item in the queue, look up the *currently resolved* version
///    (which may have been bumped higher by another path).
/// 3. If that exact (group, artifact, version) has already had its metadata
///    fetched, skip it (cycle / duplicate guard).
/// 4. Fetch and parse the POM or Gradle module file.
/// 5. For each transitive dep, apply scope mediation; if it's new or its
///    version is higher, update the resolved map and enqueue for fetching.
/// 6. After BFS, fetch all JARs and assemble classpaths and lock entries.
fn resolve_fresh(gctx: &GlobalContext, direct_deps: &[Dependency]) -> Result<ResolvedDeps> {
    // (group, artifact) → (highest_version, effective_scope)
    let mut resolved: HashMap<(String, String), (String, TransitiveScope)> = HashMap::new();
    // Guards against fetching the same (group, artifact, version) twice.
    let mut fetched: HashSet<(String, String, String)> = HashSet::new();
    let mut queue: VecDeque<(String, String, String, TransitiveScope)> = VecDeque::new();

    // Seed from direct dependencies.
    for dep in direct_deps {
        let scope = from_manifest_scope(&dep.scope);
        let key = (dep.group.clone(), dep.artifact.clone());
        update_resolved(&mut resolved, key, dep.version.clone(), scope);
        queue.push_back((
            dep.group.clone(),
            dep.artifact.clone(),
            dep.version.clone(),
            scope,
        ));
    }

    // BFS.
    while let Some((group, artifact, _, _)) = queue.pop_front() {
        let key = (group.clone(), artifact.clone());
        let (version, scope) = resolved[&key].clone();

        // Skip if we've already fetched metadata for this exact version.
        let fetch_key = (group.clone(), artifact.clone(), version.clone());
        if fetched.contains(&fetch_key) {
            continue;
        }
        fetched.insert(fetch_key);

        // Fetch POM or .module from Maven Central (cached after first download).
        vprintln!(
            gctx,
            "  [verbose] resolving metadata: {}:{}:{}",
            group,
            artifact,
            version
        );
        let metadata = cache::fetch_metadata(gctx, &group, &artifact, &version)
            .with_context(|| format!("failed to resolve {}:{}:{}", group, artifact, version))?;

        // Parse transitive deps from whichever format was returned.
        let transitives: Vec<TransitiveDep> = match metadata.format {
            MetadataFormat::Module => gradle_module::parse_module(&metadata.path)
                .with_context(|| format!("failed to parse .module for {}:{}", group, artifact))?,
            MetadataFormat::Pom => pom_transitive_deps(gctx, &metadata.path)
                .with_context(|| format!("failed to parse POM for {}:{}", group, artifact))?,
        };

        vprintln!(
            gctx,
            "  [verbose]   {} transitive dep(s) from {}:{}",
            transitives.len(),
            group,
            artifact
        );

        for trans in transitives {
            let child_scope = mediate_scope(scope, &trans.scope);

            let trans_key = (trans.group.clone(), trans.artifact.clone());
            let needs_fetch =
                update_resolved(&mut resolved, trans_key, trans.version.clone(), child_scope);

            if needs_fetch {
                queue.push_back((
                    trans.group.clone(),
                    trans.artifact.clone(),
                    trans.version.clone(),
                    child_scope,
                ));
            }
        }
    }

    // Collect, sort for determinism, fetch JARs, build output.
    let mut entries: Vec<_> = resolved.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut compile_jars = Vec::new();
    let mut runtime_jars = Vec::new();
    let mut lock_entries = Vec::new();

    vprintln!(
        gctx,
        "  [verbose] BFS complete: {} dep(s) resolved",
        entries.len()
    );

    for ((group, artifact), (version, scope)) in entries {
        vprintln!(
            gctx,
            "  [verbose] fetching JAR: {}:{}:{}",
            group,
            artifact,
            version
        );
        let (jar_path, sha256) =
            cache::fetch_jar(gctx, &group, &artifact, &version).with_context(|| {
                format!("failed to fetch JAR for {}:{}:{}", group, artifact, version)
            })?;

        match scope {
            TransitiveScope::Compile => {
                compile_jars.push(jar_path.clone());
                runtime_jars.push(jar_path);
            }
            TransitiveScope::Runtime => {
                runtime_jars.push(jar_path);
            }
        }

        lock_entries.push(LockedDependency {
            group,
            artifact,
            version,
            scope: scope_str(scope),
            sha256,
        });
    }

    Ok(ResolvedDeps {
        compile_jars,
        runtime_jars,
        lock_entries,
    })
}

// --- Phase 2 POM resolution ---

/// Resolve transitive dependencies from a POM file, applying Phase 2 features:
/// parent chain resolution, `${property}` substitution, and `<dependencyManagement>`
/// version lookup.
fn pom_transitive_deps(
    gctx: &GlobalContext,
    metadata_path: &std::path::Path,
) -> Result<Vec<TransitiveDep>> {
    let raw = crate::pom::parse_pom_raw(metadata_path)?;
    let effective = build_effective_pom(gctx, &raw, 0)?;

    let mut result = Vec::new();
    for dep in &raw.direct_deps {
        if dep.optional {
            continue;
        }

        let g = substitute_props(&dep.group, &effective.props);
        let a = substitute_props(&dep.artifact, &effective.props);

        // Resolve version: use explicit (possibly ${...}) version, or look up in managed.
        let raw_version = if dep.version.is_empty() {
            effective
                .managed
                .get(&(g.clone(), a.clone()))
                .map(|m| m.version.clone())
                .unwrap_or_default()
        } else {
            dep.version.clone()
        };
        let v = substitute_props(&raw_version, &effective.props);

        // Skip deps whose version is still unresolved.
        if v.is_empty() || v.contains("${") {
            continue;
        }

        // Resolve scope: use dep's explicit scope, or fall back to managed scope.
        let raw_scope = if dep.scope.is_empty() {
            effective
                .managed
                .get(&(g.clone(), a.clone()))
                .map(|m| m.scope.clone())
                .unwrap_or_default()
        } else {
            dep.scope.clone()
        };
        let scope = match raw_scope.as_str() {
            "" | "compile" => TransitiveScope::Compile,
            "runtime" => TransitiveScope::Runtime,
            _ => continue, // test, provided, system
        };

        result.push(TransitiveDep {
            group: g,
            artifact: a,
            version: v,
            scope,
        });
    }

    Ok(result)
}

/// The merged result of walking a POM's parent chain.
struct EffectivePom {
    group: String,
    version: String,
    props: HashMap<String, String>,
    managed: HashMap<(String, String), crate::pom::ManagedEntry>,
}

/// Follow the parent POM chain and build the merged (effective) properties and
/// `<dependencyManagement>` map for the given POM.
///
/// Child properties and managed entries override those inherited from parents.
fn build_effective_pom(gctx: &GlobalContext, pom: &ParsedPom, depth: u8) -> Result<EffectivePom> {
    const MAX_DEPTH: u8 = 10;
    if depth > MAX_DEPTH {
        anyhow::bail!(
            "parent POM chain depth exceeded {} levels (possible cycle)",
            MAX_DEPTH
        );
    }

    // Recurse into parent if present, starting with empty base values.
    let mut parent_group = String::new();
    let mut parent_version = String::new();
    let mut merged_props: HashMap<String, String> = HashMap::new();
    let mut merged_managed: HashMap<(String, String), crate::pom::ManagedEntry> = HashMap::new();

    if let Some(parent_ref) = &pom.parent {
        if !parent_ref.version.is_empty() {
            vprintln!(
                gctx,
                "  [verbose]   resolving parent POM {}:{}:{}",
                parent_ref.group,
                parent_ref.artifact,
                parent_ref.version
            );
            let parent_path = cache::fetch_pom(
                gctx,
                &parent_ref.group,
                &parent_ref.artifact,
                &parent_ref.version,
            )
            .with_context(|| {
                format!(
                    "failed to fetch parent POM {}:{}:{}",
                    parent_ref.group, parent_ref.artifact, parent_ref.version
                )
            })?;
            let parent_pom = crate::pom::parse_pom_raw(&parent_path).with_context(|| {
                format!(
                    "failed to parse parent POM {}:{}:{}",
                    parent_ref.group, parent_ref.artifact, parent_ref.version
                )
            })?;
            let parent = build_effective_pom(gctx, &parent_pom, depth + 1)?;
            parent_group = parent.group;
            parent_version = parent.version;
            merged_props = parent.props;
            merged_managed = parent.managed;
        }
    }

    // Effective coordinates: child overrides parent.
    let effective_group = if pom.group.is_empty() {
        parent_group
    } else {
        pom.group.clone()
    };
    let effective_version = if pom.version.is_empty() {
        parent_version
    } else {
        pom.version.clone()
    };

    // Merge properties: child overrides parent.
    for (k, v) in &pom.properties {
        merged_props.insert(k.clone(), v.clone());
    }

    // Resolve the effective version (may reference properties like ${revision}).
    let resolved_version = substitute_props(&effective_version, &merged_props);

    // Add built-in project.* properties after substitution.
    merged_props.insert("project.groupId".to_string(), effective_group.clone());
    merged_props.insert("project.artifactId".to_string(), pom.artifact.clone());
    merged_props.insert("project.version".to_string(), resolved_version.clone());
    if let Some(parent_ref) = &pom.parent {
        merged_props.insert(
            "project.parent.version".to_string(),
            parent_ref.version.clone(),
        );
    }

    // Merge managed deps: child overrides parent.
    for (k, v) in &pom.managed {
        merged_managed.insert(k.clone(), v.clone());
    }

    Ok(EffectivePom {
        group: effective_group,
        version: resolved_version,
        props: merged_props,
        managed: merged_managed,
    })
}

/// Replace all `${key}` placeholders in `s` with values from `props`.
///
/// Applies substitution in a loop to handle chained references (e.g., a property
/// value that itself contains `${other}`). Stops after 20 iterations to guard
/// against circular references.
fn substitute_props(s: &str, props: &HashMap<String, String>) -> String {
    let mut result = s.to_string();
    for _ in 0..20 {
        match result.find("${") {
            None => break,
            Some(start) => match result[start..].find('}') {
                None => break,
                Some(end_rel) => {
                    let end = start + end_rel;
                    let key = result[start + 2..end].to_string();
                    match props.get(&key) {
                        Some(val) => {
                            result = format!("{}{}{}", &result[..start], val, &result[end + 1..]);
                        }
                        None => break, // unknown property — leave as-is
                    }
                }
            },
        }
    }
    result
}

// --- Helpers ---

/// Update the resolved map for `key` with `(version, scope)`.
///
/// Updates when:
/// - The key is new (not yet in the map), OR
/// - `version` is higher than the current resolved version, OR
/// - `scope` is higher (Compile > Runtime) than the current scope.
///
/// Returns `true` when the *version* changed and the dep's transitives must
/// be (re-)fetched.
fn update_resolved(
    resolved: &mut HashMap<(String, String), (String, TransitiveScope)>,
    key: (String, String),
    version: String,
    scope: TransitiveScope,
) -> bool {
    match resolved.get(&key) {
        None => {
            resolved.insert(key, (version, scope));
            true
        }
        Some((existing_version, existing_scope)) => {
            let version_higher = version_gt(&version, existing_version);
            let new_version = if version_higher {
                version.clone()
            } else {
                existing_version.clone()
            };
            let new_scope = higher_scope(scope, *existing_scope);

            if version_higher || new_scope != *existing_scope {
                resolved.insert(key, (new_version, new_scope));
            }

            // Only need to re-fetch transitives if the version actually changed.
            version_higher
        }
    }
}

/// Apply the Maven scope mediation table.
///
/// | Parent scope | Child scope | Effective scope |
/// |-------------|-------------|-----------------|
/// | compile     | compile     | compile         |
/// | compile     | runtime     | runtime         |
/// | runtime     | compile     | runtime         |
/// | runtime     | runtime     | runtime         |
///
/// `provided` / `test` transitives were already filtered by the POM parser.
fn mediate_scope(parent: TransitiveScope, child: &TransitiveScope) -> TransitiveScope {
    match (parent, child) {
        (TransitiveScope::Compile, TransitiveScope::Compile) => TransitiveScope::Compile,
        _ => TransitiveScope::Runtime,
    }
}

/// Return the higher-priority scope (Compile > Runtime).
fn higher_scope(a: TransitiveScope, b: TransitiveScope) -> TransitiveScope {
    if a == TransitiveScope::Compile || b == TransitiveScope::Compile {
        TransitiveScope::Compile
    } else {
        TransitiveScope::Runtime
    }
}

fn from_manifest_scope(scope: &Scope) -> TransitiveScope {
    match scope {
        Scope::Compile => TransitiveScope::Compile,
        Scope::Runtime => TransitiveScope::Runtime,
    }
}

fn scope_str(scope: TransitiveScope) -> String {
    match scope {
        TransitiveScope::Compile => "compile".to_string(),
        TransitiveScope::Runtime => "runtime".to_string(),
    }
}

// --- Version comparison ---

/// Return `true` if version `a` is strictly greater than version `b`.
pub fn version_gt(a: &str, b: &str) -> bool {
    compare_versions(a, b) == std::cmp::Ordering::Greater
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let a_segs = version_segments(a);
    let b_segs = version_segments(b);
    let len = a_segs.len().max(b_segs.len());

    for i in 0..len {
        let a_seg = a_segs.get(i).map(String::as_str).unwrap_or("0");
        let b_seg = b_segs.get(i).map(String::as_str).unwrap_or("0");
        match compare_segment(a_seg, b_seg) {
            std::cmp::Ordering::Equal => continue,
            ord => return ord,
        }
    }

    std::cmp::Ordering::Equal
}

fn version_segments(v: &str) -> Vec<String> {
    v.split(['.', '-'])
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn compare_segment(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(an), Ok(bn)) => an.cmp(&bn),
        // Numeric segment vs qualifier: numeric is a release, qualifier is pre-release.
        (Ok(_), Err(_)) => std::cmp::Ordering::Greater,
        (Err(_), Ok(_)) => std::cmp::Ordering::Less,
        (Err(_), Err(_)) => qualifier_rank(a)
            .cmp(&qualifier_rank(b))
            .then_with(|| a.cmp(b)),
    }
}

/// Lower rank = lower version. Non-release qualifiers are negative.
/// Trailing digits are stripped so "RC1", "BETA2" etc. rank the same as "RC", "BETA".
fn qualifier_rank(q: &str) -> i32 {
    let base = q.trim_end_matches(|c: char| c.is_ascii_digit());
    match base.to_ascii_uppercase().as_str() {
        "SNAPSHOT" => -4,
        "ALPHA" => -3,
        "BETA" => -2,
        "RC" => -1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Version comparison ---

    #[test]
    fn test_version_gt_numeric() {
        assert!(version_gt("1.2.4", "1.2.3"));
        assert!(version_gt("1.10.0", "1.9.0")); // numeric, not lexicographic
        assert!(version_gt("2.0.0", "1.9.9"));
        assert!(!version_gt("1.2.3", "1.2.3")); // equal
        assert!(!version_gt("1.2.2", "1.2.3")); // lower
    }

    #[test]
    fn test_version_gt_snapshot() {
        assert!(version_gt("1.0.0", "1.0.0-SNAPSHOT"));
        assert!(!version_gt("1.0.0-SNAPSHOT", "1.0.0"));
    }

    #[test]
    fn test_version_gt_qualifiers() {
        assert!(version_gt("1.0.0", "1.0.0-RC1"));
        assert!(version_gt("1.0.0-RC1", "1.0.0-BETA1"));
        assert!(version_gt("1.0.0-BETA1", "1.0.0-ALPHA1"));
        assert!(version_gt("1.0.0-RC1", "1.0.0-SNAPSHOT"));
    }

    #[test]
    fn test_version_segments() {
        assert_eq!(version_segments("1.2.3"), vec!["1", "2", "3"]);
        assert_eq!(version_segments("33.0.0-jre"), vec!["33", "0", "0", "jre"]);
        assert_eq!(
            version_segments("1.0.0-SNAPSHOT"),
            vec!["1", "0", "0", "SNAPSHOT"]
        );
    }

    // --- Scope mediation ---

    #[test]
    fn test_mediate_scope() {
        use TransitiveScope::*;
        assert_eq!(mediate_scope(Compile, &Compile), Compile);
        assert_eq!(mediate_scope(Compile, &Runtime), Runtime);
        assert_eq!(mediate_scope(Runtime, &Compile), Runtime);
        assert_eq!(mediate_scope(Runtime, &Runtime), Runtime);
    }

    #[test]
    fn test_higher_scope() {
        use TransitiveScope::*;
        assert_eq!(higher_scope(Compile, Runtime), Compile);
        assert_eq!(higher_scope(Runtime, Compile), Compile);
        assert_eq!(higher_scope(Compile, Compile), Compile);
        assert_eq!(higher_scope(Runtime, Runtime), Runtime);
    }

    // --- update_resolved ---

    #[test]
    fn test_update_resolved_new_dep() {
        let mut resolved = HashMap::new();
        let key = ("com.example".to_string(), "foo".to_string());
        let needs_fetch = update_resolved(
            &mut resolved,
            key.clone(),
            "1.0.0".to_string(),
            TransitiveScope::Compile,
        );
        assert!(needs_fetch);
        assert_eq!(resolved[&key].0, "1.0.0");
        assert_eq!(resolved[&key].1, TransitiveScope::Compile);
    }

    #[test]
    fn test_update_resolved_higher_version_wins() {
        let mut resolved = HashMap::new();
        let key = ("com.example".to_string(), "foo".to_string());
        update_resolved(
            &mut resolved,
            key.clone(),
            "1.0.0".to_string(),
            TransitiveScope::Compile,
        );
        let needs_fetch = update_resolved(
            &mut resolved,
            key.clone(),
            "2.0.0".to_string(),
            TransitiveScope::Compile,
        );
        assert!(needs_fetch);
        assert_eq!(resolved[&key].0, "2.0.0");
    }

    #[test]
    fn test_update_resolved_lower_version_ignored() {
        let mut resolved = HashMap::new();
        let key = ("com.example".to_string(), "foo".to_string());
        update_resolved(
            &mut resolved,
            key.clone(),
            "2.0.0".to_string(),
            TransitiveScope::Compile,
        );
        let needs_fetch = update_resolved(
            &mut resolved,
            key.clone(),
            "1.0.0".to_string(),
            TransitiveScope::Compile,
        );
        assert!(!needs_fetch);
        assert_eq!(resolved[&key].0, "2.0.0"); // unchanged
    }

    #[test]
    fn test_update_resolved_scope_upgraded() {
        let mut resolved = HashMap::new();
        let key = ("com.example".to_string(), "foo".to_string());
        update_resolved(
            &mut resolved,
            key.clone(),
            "1.0.0".to_string(),
            TransitiveScope::Runtime,
        );
        // Same version but Compile scope → upgrade
        let needs_fetch = update_resolved(
            &mut resolved,
            key.clone(),
            "1.0.0".to_string(),
            TransitiveScope::Compile,
        );
        assert!(!needs_fetch); // version didn't change, no re-fetch needed
        assert_eq!(resolved[&key].1, TransitiveScope::Compile); // scope upgraded
    }

    // --- lock_is_fresh ---

    fn make_dep(group: &str, artifact: &str, version: &str) -> Dependency {
        Dependency {
            group: group.to_string(),
            artifact: artifact.to_string(),
            version: version.to_string(),
            scope: Scope::Compile,
            expose: false,
        }
    }

    fn make_lock_entry(group: &str, artifact: &str, version: &str) -> LockedDependency {
        LockedDependency {
            group: group.to_string(),
            artifact: artifact.to_string(),
            version: version.to_string(),
            scope: "compile".to_string(),
            sha256: "abc123".to_string(),
        }
    }

    #[test]
    fn test_lock_is_fresh_all_match() {
        let deps = vec![make_dep("com.example", "foo", "1.0.0")];
        let lock = LockFile {
            dependency: vec![make_lock_entry("com.example", "foo", "1.0.0")],
        };
        assert!(lock_is_fresh(&deps, &lock));
    }

    #[test]
    fn test_lock_is_fresh_dep_missing_from_lock() {
        let deps = vec![
            make_dep("com.example", "foo", "1.0.0"),
            make_dep("com.example", "bar", "2.0.0"),
        ];
        let lock = LockFile {
            dependency: vec![make_lock_entry("com.example", "foo", "1.0.0")],
        };
        assert!(!lock_is_fresh(&deps, &lock));
    }

    #[test]
    fn test_lock_is_fresh_version_changed() {
        let deps = vec![make_dep("com.example", "foo", "2.0.0")];
        let lock = LockFile {
            dependency: vec![make_lock_entry("com.example", "foo", "1.0.0")],
        };
        assert!(!lock_is_fresh(&deps, &lock));
    }

    #[test]
    fn test_lock_is_fresh_extra_transitive_in_lock_is_ok() {
        // Lock may have transitive deps not in the manifest — that's fine.
        let deps = vec![make_dep("com.example", "foo", "1.0.0")];
        let lock = LockFile {
            dependency: vec![
                make_lock_entry("com.example", "foo", "1.0.0"),
                make_lock_entry("org.other", "transitive", "3.0.0"),
            ],
        };
        assert!(lock_is_fresh(&deps, &lock));
    }

    #[test]
    fn test_lock_is_fresh_empty_deps() {
        let lock = LockFile {
            dependency: vec![make_lock_entry("com.example", "foo", "1.0.0")],
        };
        assert!(lock_is_fresh(&[], &lock));
    }

    // --- substitute_props ---

    #[test]
    fn test_substitute_props_simple() {
        let mut props = HashMap::new();
        props.insert("foo.version".to_string(), "1.2.3".to_string());
        assert_eq!(substitute_props("${foo.version}", &props), "1.2.3");
        assert_eq!(
            substitute_props("prefix-${foo.version}-suffix", &props),
            "prefix-1.2.3-suffix"
        );
    }

    #[test]
    fn test_substitute_props_no_placeholder() {
        let props = HashMap::new();
        assert_eq!(substitute_props("1.0.0", &props), "1.0.0");
        assert_eq!(substitute_props("", &props), "");
    }

    #[test]
    fn test_substitute_props_unknown_property() {
        let props = HashMap::new();
        // Unknown property — left as-is
        assert_eq!(substitute_props("${unknown}", &props), "${unknown}");
    }

    #[test]
    fn test_substitute_props_chained() {
        let mut props = HashMap::new();
        // a references b, b has a concrete value
        props.insert("a".to_string(), "${b}".to_string());
        props.insert("b".to_string(), "final".to_string());
        assert_eq!(substitute_props("${a}", &props), "final");
    }

    #[test]
    fn test_substitute_props_project_version() {
        let mut props = HashMap::new();
        props.insert("project.version".to_string(), "3.0.0".to_string());
        assert_eq!(substitute_props("${project.version}", &props), "3.0.0");
    }

    #[test]
    fn test_substitute_props_multiple_in_one_string() {
        let mut props = HashMap::new();
        props.insert("g".to_string(), "com.example".to_string());
        props.insert("v".to_string(), "1.0".to_string());
        // Only one placeholder is replaced per iteration, but the loop handles both
        assert_eq!(
            substitute_props("${g}:lib:${v}", &props),
            "com.example:lib:1.0"
        );
    }

    // --- pom_transitive_deps (unit tests using temp files, no network) ---

    fn make_test_gctx(tmp: &tempfile::TempDir) -> crate::context::GlobalContext {
        crate::context::GlobalContext {
            verbose: false,
            cwd: tmp.path().to_path_buf(),
            jargo_home: tmp.path().join(".jargo"),
        }
    }

    #[test]
    fn test_pom_transitive_deps_with_property_version() {
        use std::fs;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let gctx = make_test_gctx(&tmp);
        let pom_path = tmp.path().join("test.pom");
        let xml = r#"<?xml version="1.0"?>
<project>
  <groupId>com.example</groupId>
  <artifactId>parent-style</artifactId>
  <version>2.0.0</version>
  <properties>
    <dep.version>1.5.0</dep.version>
  </properties>
  <dependencies>
    <dependency>
      <groupId>org.apache.commons</groupId>
      <artifactId>commons-lang3</artifactId>
      <version>${dep.version}</version>
    </dependency>
  </dependencies>
</project>"#;
        fs::write(&pom_path, xml).unwrap();
        let deps = pom_transitive_deps(&gctx, &pom_path).unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].artifact, "commons-lang3");
        assert_eq!(deps[0].version, "1.5.0");
    }

    #[test]
    fn test_pom_transitive_deps_managed_version_fills_in() {
        use std::fs;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let gctx = make_test_gctx(&tmp);
        let pom_path = tmp.path().join("test.pom");
        let xml = r#"<?xml version="1.0"?>
<project>
  <groupId>com.example</groupId>
  <artifactId>bom-user</artifactId>
  <version>1.0.0</version>
  <dependencyManagement>
    <dependencies>
      <dependency>
        <groupId>org.example</groupId>
        <artifactId>foo</artifactId>
        <version>3.2.1</version>
      </dependency>
    </dependencies>
  </dependencyManagement>
  <dependencies>
    <dependency>
      <groupId>org.example</groupId>
      <artifactId>foo</artifactId>
    </dependency>
  </dependencies>
</project>"#;
        fs::write(&pom_path, xml).unwrap();
        let deps = pom_transitive_deps(&gctx, &pom_path).unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].group, "org.example");
        assert_eq!(deps[0].artifact, "foo");
        assert_eq!(deps[0].version, "3.2.1");
        assert_eq!(deps[0].scope, TransitiveScope::Compile);
    }

    #[test]
    fn test_pom_transitive_deps_project_version_substitution() {
        use std::fs;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let gctx = make_test_gctx(&tmp);
        let pom_path = tmp.path().join("test.pom");
        // A common pattern: BOM-style POM where managed versions reference ${project.version}
        let xml = r#"<?xml version="1.0"?>
<project>
  <groupId>com.example</groupId>
  <artifactId>my-bom</artifactId>
  <version>5.0.0</version>
  <dependencyManagement>
    <dependencies>
      <dependency>
        <groupId>com.example</groupId>
        <artifactId>module-a</artifactId>
        <version>${project.version}</version>
      </dependency>
    </dependencies>
  </dependencyManagement>
  <dependencies>
    <dependency>
      <groupId>com.example</groupId>
      <artifactId>module-a</artifactId>
    </dependency>
  </dependencies>
</project>"#;
        fs::write(&pom_path, xml).unwrap();
        let deps = pom_transitive_deps(&gctx, &pom_path).unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].version, "5.0.0");
    }

    #[test]
    fn test_pom_transitive_deps_still_unversioned_skipped() {
        use std::fs;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let gctx = make_test_gctx(&tmp);
        let pom_path = tmp.path().join("test.pom");
        // A dep with no version and no managed entry — should be skipped
        let xml = r#"<?xml version="1.0"?>
<project>
  <groupId>com.example</groupId>
  <artifactId>test-pom</artifactId>
  <version>1.0.0</version>
  <dependencies>
    <dependency>
      <groupId>com.example</groupId>
      <artifactId>no-version</artifactId>
    </dependency>
    <dependency>
      <groupId>com.example</groupId>
      <artifactId>has-version</artifactId>
      <version>2.0.0</version>
    </dependency>
  </dependencies>
</project>"#;
        fs::write(&pom_path, xml).unwrap();
        let deps = pom_transitive_deps(&gctx, &pom_path).unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].artifact, "has-version");
    }
}
