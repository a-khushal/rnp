use reqwest;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use semver::{Version, VersionReq};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::error::Error;
use std::sync::Arc;
use crate::cache::PackageCache;
use tokio::sync::Semaphore;
use tar;
use flate2;
use std::fs;
use std::path::Path;
use std::io::Cursor;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use sha2::{Digest, Sha512};

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub no_package_lock: bool,
    pub verbose: bool,
    pub quiet: bool,
    pub ignore_scripts: bool,
    pub workspace: Option<String>,
    pub hoist_strategy: String,
}

impl InstallOptions {
    fn info(&self, message: &str) {
        if !self.quiet {
            println!("{}", message.cyan());
        }
    }

    fn success(&self, message: &str) {
        if !self.quiet {
            println!("{}", message.green());
        }
    }

    fn warn(&self, message: &str) {
        if !self.quiet {
            eprintln!("{}", message.yellow());
        }
    }

    fn debug(&self, message: &str) {
        if self.verbose && !self.quiet {
            println!("{}", message.dimmed());
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PackageLock {
    name: String,
    version: String,
    #[serde(rename = "lockfileVersion")]
    lockfile_version: u8,
    requires: bool,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    dependencies: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    workspace_paths: BTreeMap<String, String>,
    packages: BTreeMap<String, LockfilePackage>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LockfilePackage {
    version: String,
    resolved: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    integrity: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    dependencies: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    shasum: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NpmVersionReq {
    raw: String,
    clauses: Vec<VersionReq>,
}

impl NpmVersionReq {
    fn parse(input: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let raw = if input.trim().is_empty() { "*" } else { input.trim() }.to_string();
        let mut clauses = Vec::new();

        for clause in raw.split("||") {
            let normalized = normalize_npm_clause(clause.trim());
            clauses.push(VersionReq::parse(&normalized)?);
        }

        if clauses.is_empty() {
            clauses.push(VersionReq::parse("*")?);
        }

        Ok(Self { raw, clauses })
    }

    fn any() -> Result<Self, Box<dyn Error + Send + Sync>> {
        Self::parse("*")
    }

    fn matches(&self, version: &Version) -> bool {
        self.clauses.iter().any(|req| req.matches(version))
    }

    fn display(&self) -> String {
        self.raw.clone()
    }
}

impl std::fmt::Display for NpmVersionReq {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.raw)
    }
}

fn normalize_npm_clause(clause: &str) -> String {
    if clause.is_empty() || clause == "*" {
        return "*".to_string();
    }

    if let Some((start, end)) = clause.split_once(" - ") {
        return format!(">={}, <={}", start.trim(), end.trim());
    }

    if clause.contains('x') || clause.contains('X') || clause.contains('*') {
        return normalize_wildcard_clause(clause);
    }

    let mut result = String::new();
    let mut last_was_operator = false;
    for token in clause.split_whitespace() {
        if token.starts_with('<') || token.starts_with('>') {
            if !result.is_empty() {
                result.push_str(", ");
            }
            result.push_str(token);
            last_was_operator = true;
        } else {
            if last_was_operator {
                result.push_str(token);
            } else {
                if !result.is_empty() {
                    result.push_str(", ");
                }
                result.push_str(token);
            }
            last_was_operator = false;
        }
    }

    if result.is_empty() {
        clause.to_string()
    } else {
        result
    }
}

fn normalize_wildcard_clause(clause: &str) -> String {
    let trimmed = clause.trim();
    if trimmed == "*" || trimmed.eq_ignore_ascii_case("x") {
        return "*".to_string();
    }

    let parts: Vec<&str> = trimmed.split('.').collect();
    let major = parts.first().copied().unwrap_or("0");
    let minor = parts.get(1).copied().unwrap_or("x");
    let patch = parts.get(2).copied().unwrap_or("x");

    let is_wild = |v: &str| v == "*" || v.eq_ignore_ascii_case("x");

    if is_wild(major) {
        return "*".to_string();
    }

    if is_wild(minor) {
        let major_num = major.parse::<u64>().unwrap_or(0);
        return format!(">={}.0.0, <{}.0.0", major_num, major_num + 1);
    }

    let major_num = major.parse::<u64>().unwrap_or(0);
    let minor_num = minor.parse::<u64>().unwrap_or(0);

    if is_wild(patch) {
        return format!(">={}.{}.0, <{}.{}.0", major_num, minor_num, major_num, minor_num + 1);
    }

    clause.to_string()
}


#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub name: String,
    pub version: Version,
    pub dependencies: HashMap<String, NpmVersionReq>,
    pub peer_dependencies: HashMap<String, NpmVersionReq>,
    pub optional_dependencies: HashMap<String, NpmVersionReq>,
    pub tarball_url: String,
    pub integrity: Option<String>,
    pub shasum: Option<String>,
    pub is_workspace: bool,
    pub workspace_path: Option<PathBuf>,
    pub engines_node: Option<NpmVersionReq>,
    pub os_constraints: Vec<String>,
    pub cpu_constraints: Vec<String>,
    pub lifecycle_scripts: HashMap<String, String>,
    pub bin_entries: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    pub info: PackageInfo,
    pub depth: usize,
    pub optional: bool,
}

pub struct DependencyResolver {
    registry_client: Arc<reqwest::Client>,
    conflicts: Vec<String>,
    workspace_packages: HashMap<String, WorkspacePackage>,
}

#[derive(Debug, Clone)]
struct WorkspacePackage {
    version: Version,
    path: PathBuf,
}

impl DependencyResolver {
    fn new(workspace_packages: HashMap<String, WorkspacePackage>) -> Self {
        Self {
            registry_client: Arc::new(reqwest::Client::new()),
            conflicts: Vec::new(),
            workspace_packages,
        }
    }

    // Phase-1: Build complete dependency graph
    /*
        * Box<...> -> A heap-allocated smart pointer (owned, single owner).
        * dyn std::error::Error -> A trait object that can hold any type implementing the std::error::Error trait.
        * Send -> The error type can be safely sent between threads.
        * Sync -> The error type can be safely shared between threads.
    */
    pub async fn resolve_dependencies(
        &mut self,
        root_package: &str,
        locked_versions: Option<&HashMap<String, Version>>,
    ) -> Result<Vec<ResolvedPackage>, Box<dyn std::error::Error + Send + Sync>> { 
        // local variable to store the packages to resolve
        let mut to_resolve: VecDeque<(String, NpmVersionReq, usize, bool)> = VecDeque::new();
        // local variable to store the resolved packages
        let mut resolved: HashMap<String, (Version, usize)> = HashMap::new();
        // local variable to store the resolved packages
        let mut resolved_packages: HashMap<String, ResolvedPackage> = HashMap::new();
        
        // push the root package to the to_resolve queue
        to_resolve.push_back((root_package.to_string(), NpmVersionReq::any()?, 0, false));

        // classic BFS
        while let Some((package_name, version_req, depth, is_optional)) = to_resolve.pop_front() {
            // if the package is already resolved, skip it
            if let Some((existing_version_req, existing_depth)) = resolved.get(&package_name) {
                // if the version requirement matches, skip it
                if version_req.matches(existing_version_req) {
                    continue;
                } 

                // if the depth is less than or equal to the existing depth, skip it
                if depth <= *existing_depth {
                    self.conflicts.push(format!(
                        "Version conflict for {}: {} vs {}",
                        package_name,
                        version_req.display(),
                        existing_version_req
                    ));
                    continue;
                }
            }

            // fetch the package metadata
            let locked_version = locked_versions.and_then(|m| m.get(&package_name));
            let package_info = match self
                .fetch_package_metadata(&package_name, &version_req, locked_version)
                .await
            {
                Ok(info) => info,
                Err(err) if is_optional => {
                    self.conflicts.push(format!(
                        "Skipping optional dependency {} ({}): {}",
                        package_name,
                        version_req.display(),
                        err
                    ));
                    continue;
                }
                Err(err) => return Err(err),
            };

            // insert the package into the resolved map
            resolved.insert(package_name.clone(), (package_info.version.clone(), depth));

            // insert the package into the resolved packages map which is global
            resolved_packages.insert(
                package_name.clone(),
                ResolvedPackage {
                    info: package_info.clone(),
                    depth,
                    optional: is_optional,
                },
            );

            // push the dependencies to the to_resolve queue
            for (dep_name, dep_version_req) in &package_info.dependencies {
                to_resolve.push_back((dep_name.clone(), dep_version_req.clone(), depth + 1, false));
            }

            // push peer dependencies as well
            for (peer_name, peer_version_req) in &package_info.peer_dependencies {
                to_resolve.push_back((peer_name.clone(), peer_version_req.clone(), depth + 1, false));
            }

            for (opt_name, opt_version_req) in &package_info.optional_dependencies {
                to_resolve.push_back((opt_name.clone(), opt_version_req.clone(), depth + 1, true));
            }
        }

        // return the resolved packages from the global map
        let mut packages = resolved_packages.values().cloned().collect::<Vec<_>>();
        packages.sort_by_key(|p| p.depth);
        Ok(packages)
    }
 
    // Fetch package metadata from the npm registry
    async fn fetch_package_metadata(
        &self,
        name: &str,
        version_req: &NpmVersionReq,
        locked_version: Option<&Version>,
    ) -> Result<PackageInfo, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(workspace_pkg) = self.workspace_packages.get(name) {
            if version_req.matches(&workspace_pkg.version) {
                return Ok(PackageInfo {
                    name: name.to_string(),
                    version: workspace_pkg.version.clone(),
                    dependencies: HashMap::new(),
                    peer_dependencies: HashMap::new(),
                    optional_dependencies: HashMap::new(),
                    tarball_url: String::new(),
                    integrity: None,
                    shasum: None,
                    is_workspace: true,
                    workspace_path: Some(workspace_pkg.path.clone()),
                    engines_node: None,
                    os_constraints: Vec::new(),
                    cpu_constraints: Vec::new(),
                    lifecycle_scripts: HashMap::new(),
                    bin_entries: HashMap::new(),
                });
            }
        }

        let url = format!("https://registry.npmjs.org/{}", name);
        let response = self.registry_client.get(&url).send().await?;
        let metadata: serde_json::Value = response.json().await?;

        // Find best matching version
        let versions = metadata["versions"]
            .as_object()
            .ok_or("No versions found")?;

        let best_version = self.find_best_version(versions.keys(), version_req, locked_version)?;
        let version_info = &metadata["versions"][&best_version.to_string()];

        // Parse dependencies
        let mut dependencies = HashMap::new();
        if let Some(deps) = version_info.get("dependencies") {
            if let Some(deps_obj) = deps.as_object() {
                for (dep_name, dep_version) in deps_obj {
                    if let Some(version_str) = dep_version.as_str() {
                        match NpmVersionReq::parse(version_str) {
                            Ok(req) => {
                                dependencies.insert(dep_name.clone(), req);
                            }
                            Err(e) => {
                                println!(
                                    "⚠️  Warning: Could not parse version requirement for '{}': '{}'. Error: {}. Using '*' as fallback.",
                                    dep_name, version_str, e
                                );
                                if let Ok(any_version_req) = NpmVersionReq::any() {
                                    dependencies.insert(dep_name.clone(), any_version_req);
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut peer_dependencies = HashMap::new();
        if let Some(peer_deps) = version_info.get("peerDependencies") {
            if let Some(peer_deps_obj) = peer_deps.as_object() {
                for (dep_name, dep_version) in peer_deps_obj {
                    if let Some(version_str) = dep_version.as_str() {
                        match NpmVersionReq::parse(version_str) {
                            Ok(req) => {
                                peer_dependencies.insert(dep_name.clone(), req);
                            }
                            Err(e) => {
                                println!(
                                    "⚠️  Warning: Could not parse peer dependency for '{}': '{}'. Error: {}. Using '*' as fallback.",
                                    dep_name, version_str, e
                                );
                                if let Ok(any_version_req) = NpmVersionReq::any() {
                                    peer_dependencies.insert(dep_name.clone(), any_version_req);
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut optional_dependencies = HashMap::new();
        if let Some(optional_deps) = version_info.get("optionalDependencies") {
            if let Some(optional_deps_obj) = optional_deps.as_object() {
                for (dep_name, dep_version) in optional_deps_obj {
                    if let Some(version_str) = dep_version.as_str() {
                        match NpmVersionReq::parse(version_str) {
                            Ok(req) => {
                                optional_dependencies.insert(dep_name.clone(), req);
                            }
                            Err(e) => {
                                println!(
                                    "⚠️  Warning: Could not parse optional dependency for '{}': '{}'. Error: {}. Using '*' as fallback.",
                                    dep_name, version_str, e
                                );
                                if let Ok(any_version_req) = NpmVersionReq::any() {
                                    optional_dependencies.insert(dep_name.clone(), any_version_req);
                                }
                            }
                        }
                    }
                }
            }
        }

        let tarball_url = version_info["dist"]["tarball"]
            .as_str()
            .ok_or("No tarball URL found")?
            .to_string();

        let shasum = version_info["dist"]["shasum"]
            .as_str()
            .map(|value| value.to_string());

        let integrity = version_info["dist"]["integrity"]
            .as_str()
            .map(|value| value.to_string());

        let engines_node = version_info
            .get("engines")
            .and_then(|v| v.get("node"))
            .and_then(|v| v.as_str())
            .and_then(|v| NpmVersionReq::parse(v).ok());

        let os_constraints = version_info
            .get("os")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let cpu_constraints = version_info
            .get("cpu")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let mut lifecycle_scripts = HashMap::new();
        if let Some(scripts_obj) = version_info.get("scripts").and_then(|v| v.as_object()) {
            for script_name in ["preinstall", "install", "postinstall"] {
                if let Some(command) = scripts_obj.get(script_name).and_then(|v| v.as_str()) {
                    lifecycle_scripts.insert(script_name.to_string(), command.to_string());
                }
            }
        }

        let mut bin_entries = HashMap::new();
        if let Some(bin) = version_info.get("bin") {
            if let Some(single_bin) = bin.as_str() {
                bin_entries.insert(default_bin_name(name), single_bin.to_string());
            } else if let Some(bin_map) = bin.as_object() {
                for (bin_name, bin_path) in bin_map {
                    if let Some(bin_path_str) = bin_path.as_str() {
                        bin_entries.insert(bin_name.clone(), bin_path_str.to_string());
                    }
                }
            }
        }

        Ok(PackageInfo {
            name: name.to_string(),
            version: best_version,
            dependencies,
            peer_dependencies,
            optional_dependencies,
            tarball_url,
            integrity,
            shasum,
            is_workspace: false,
            workspace_path: None,
            engines_node,
            os_constraints,
            cpu_constraints,
            lifecycle_scripts,
            bin_entries,
        })
    }

    fn find_best_version(
        &self,
        available_versions: serde_json::map::Keys,
        requirement: &NpmVersionReq,
        locked_version: Option<&Version>,
    ) -> Result<Version, Box<dyn std::error::Error + Send + Sync>> {
        let matching_versions: Vec<Version> = available_versions
            .filter_map(|v| Version::parse(v).ok())
            .filter(|v| requirement.matches(v))
            .collect();

        if let Some(locked) = locked_version {
            if matching_versions.iter().any(|v| v == locked) {
                return Ok(locked.clone());
            }
        }

        let mut matching_versions = matching_versions;

        matching_versions.sort_by(|a, b| b.cmp(a)); // Descending order (latest first)

        matching_versions
            .into_iter()
            .next()
            .ok_or("No matching version found".into())
    }

    // Phase 2: Parallel download and installation
    pub async fn install_packages_parallel(
        &self,
        packages: &Vec<ResolvedPackage>,
        options: &InstallOptions,
        node_version: Option<Version>,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        const MAX_CONCURRENT_DOWNLOADS: usize = 15;
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_DOWNLOADS));
        let progress = if options.quiet {
            None
        } else {
            let pb = ProgressBar::new(packages.len() as u64);
            pb.set_style(
                ProgressStyle::with_template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                    .unwrap()
                    .progress_chars("##-"),
            );
            pb.set_message("installing packages");
            Some(pb)
        };

        // Group packages by depth for proper installation order
        let mut depth_groups: HashMap<usize, Vec<&ResolvedPackage>> = HashMap::new();
        for package in packages {
            depth_groups.entry(package.depth).or_default().push(package);
        }

        // Install depth by depth (to respect dependency order)
        let mut total_installed = 0;
        let mut depths: Vec<_> = depth_groups.keys().cloned().collect();
        depths.sort_by(|a, b| b.cmp(a)); // Deepest first

        for depth in depths {
            let packages_at_depth = depth_groups.remove(&depth).unwrap();
            let mut depth_handles = Vec::new();

            // Install packages at same depth in parallel
            for package in packages_at_depth {
                let semaphore = Arc::clone(&semaphore);
                let client = Arc::clone(&self.registry_client);
                let package_to_install = package.clone();
                let node_version = node_version.clone();
                let options = options.clone();

                let handle = tokio::spawn(async move {
                    let _permit = semaphore.acquire().await.unwrap();
                    Self::download_and_extract_package(client, &package_to_install, &options, node_version)
                        .await
                });

                depth_handles.push(handle);
            }

            // Wait for all packages at this depth to complete
            for handle in depth_handles {
                if handle.await?? {
                    total_installed += 1;
                    if let Some(pb) = &progress {
                        pb.inc(1);
                    }
                }
            }
        }

        if let Some(pb) = &progress {
            pb.finish_with_message("done");
        }

        Ok(total_installed)
    }

    async fn download_and_extract_package(
        client: Arc<reqwest::Client>,
        package: &ResolvedPackage,
        options: &InstallOptions,
        node_version: Option<Version>,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        const CACHE_MAX_AGE: Duration = Duration::from_secs(60 * 60 * 24 * 7);

        if let Err(reason) = validate_package_constraints(&package.info, node_version.as_ref()) {
            if package.optional {
                options.warn(&format!("Skipping optional dependency {}: {}", package.info.name, reason));
                return Ok(false);
            }
            return Err(reason.into());
        }

        if package.info.is_workspace {
            let src = package
                .info
                .workspace_path
                .as_ref()
                .ok_or("Workspace package path not found")?;
            let node_modules_path = Path::new("node_modules").join(&package.info.name);
            if node_modules_path.exists() {
                fs::remove_dir_all(&node_modules_path)?;
            }

            if let Err(_err) = symlink_dir(src, &node_modules_path) {
                copy_dir_recursive(src, &node_modules_path)?;
            }
            return Ok(true);
        }

        // Initialize cache
        let cache = PackageCache::new()?;
        let package_version = package.info.version.to_string();

        // Check cache first
        let bytes = if let Some(cached_data) = cache.get_valid_tarball(
            &package.info.name,
            &package_version,
            package.info.shasum.as_deref(),
            CACHE_MAX_AGE,
        )? {
            if verify_tarball_integrity(&package.info, &cached_data).is_ok() {
                cached_data
            } else {
                cache.invalidate_tarball(&package.info.name, &package_version)?;
                let response = client.get(&package.info.tarball_url).send().await?;
                let bytes = response.bytes().await?;
                verify_tarball_integrity(&package.info, bytes.as_ref())
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
                if let Err(e) = cache.save_tarball(&package.info.name, &package_version, &bytes) {
                    eprintln!("  ⚠️  Failed to cache {}@{}: {}", package.info.name, package.info.version, e);
                }
                bytes.to_vec()
            }
        } else {
            // Cache miss, stale entry, or checksum mismatch: download again
            let response = client.get(&package.info.tarball_url).send().await?;
            let bytes = response.bytes().await?;

            verify_tarball_integrity(&package.info, bytes.as_ref())
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;

            // Save to cache for future use
            if let Err(e) = cache.save_tarball(&package.info.name, &package_version, &bytes) {
                eprintln!("  ⚠️  Failed to cache {}@{}: {}", package.info.name, package.info.version, e);
            }
            bytes.to_vec()
        };

        // Extract to node_modules
        let node_modules_path = Path::new("node_modules").join(&package.info.name);
        fs::create_dir_all(&node_modules_path)?;

        // Extract tarball
        /*
            * Cursor wraps the Vec<u8> and makes it behave like a file
            * Implements Read → lets libraries read bytes sequentially.
            * Implements Seek → lets libraries jump around in the data if needed.
        */
        let tar = flate2::read::GzDecoder::new(Cursor::new(bytes)); // .tar.gz -> .tar
        let mut archive = tar::Archive::new(tar); // .tar -> . i.e., each file in the tarball with proper directory structure

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.into_owned();

            let mut components = path.components();
            components.next(); // Skip top-level folder
            let relative_path = components.as_path();
            let dest_path = node_modules_path.join(relative_path);

            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }

            entry.unpack(dest_path)?;
        }

        create_bin_links(&package.info, &node_modules_path)?;
        run_lifecycle_scripts(&package.info, &node_modules_path, options)?;

        Ok(true)
    }
}

fn load_locked_versions() -> Result<HashMap<String, Version>, Box<dyn std::error::Error + Send + Sync>> {
    let path = Path::new("package-lock.json");
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let data = std::fs::read_to_string(path)?;
    let lockfile: PackageLock = serde_json::from_str(&data)?;

    let mut locked_versions = HashMap::new();
    for (path_key, locked_package) in lockfile.packages {
        let Some(name) = lockfile_package_name(&path_key) else {
            continue;
        };
        if let Ok(version) = Version::parse(&locked_package.version) {
            locked_versions.insert(name.to_string(), version);
        }
    }

    Ok(locked_versions)
}

fn lockfile_package_name(path_key: &str) -> Option<&str> {
    if path_key.is_empty() {
        return None;
    }

    if path_key.contains("node_modules/") {
        return path_key.rsplit("node_modules/").next();
    }

    Some(path_key)
}

fn expand_workspace_pattern(pattern: &str) -> Vec<PathBuf> {
    if let Some(prefix) = pattern.strip_suffix("/*") {
        let base = Path::new(prefix);
        let mut paths = Vec::new();
        if let Ok(entries) = fs::read_dir(base) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    paths.push(path);
                }
            }
        }
        return paths;
    }

    vec![PathBuf::from(pattern)]
}

fn load_workspace_packages() -> Result<HashMap<String, WorkspacePackage>, Box<dyn std::error::Error + Send + Sync>> {
    let root_package_json = Path::new("package.json");
    if !root_package_json.exists() {
        return Ok(HashMap::new());
    }

    let data = fs::read_to_string(root_package_json)?;
    let json: Value = serde_json::from_str(&data)?;
    let mut workspace_patterns: Vec<String> = Vec::new();

    if let Some(workspaces) = json.get("workspaces") {
        if let Some(arr) = workspaces.as_array() {
            for item in arr {
                if let Some(pattern) = item.as_str() {
                    workspace_patterns.push(pattern.to_string());
                }
            }
        } else if let Some(packages) = workspaces.get("packages").and_then(|v| v.as_array()) {
            for item in packages {
                if let Some(pattern) = item.as_str() {
                    workspace_patterns.push(pattern.to_string());
                }
            }
        }
    }

    let mut workspace_packages = HashMap::new();
    for pattern in workspace_patterns {
        for workspace_dir in expand_workspace_pattern(&pattern) {
            let workspace_package_json = workspace_dir.join("package.json");
            if !workspace_package_json.exists() {
                continue;
            }

            let workspace_data = fs::read_to_string(&workspace_package_json)?;
            let workspace_json: Value = serde_json::from_str(&workspace_data)?;

            let Some(name) = workspace_json.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            let version_str = workspace_json
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("1.0.0");

            if let Ok(version) = Version::parse(version_str) {
                workspace_packages.insert(
                    name.to_string(),
                    WorkspacePackage {
                        version,
                        path: workspace_dir,
                    },
                );
            }
        }
    }

    Ok(workspace_packages)
}

fn workspace_manifest_path(
    workspace_name: Option<&str>,
    workspace_packages: &HashMap<String, WorkspacePackage>,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    if let Some(name) = workspace_name {
        let workspace = workspace_packages
            .get(name)
            .ok_or_else(|| format!("Workspace '{}' not found", name))?;
        return Ok(workspace.path.join("package.json"));
    }

    Ok(PathBuf::from("package.json"))
}

fn read_manifest_dependencies_from(path: &Path) -> Result<BTreeMap<String, String>, Box<dyn std::error::Error + Send + Sync>> {
    let data = fs::read_to_string(path)?;
    let json: Value = serde_json::from_str(&data)?;
    let dependencies = json
        .get("dependencies")
        .and_then(|v| v.as_object())
        .map(|deps| {
            deps.iter()
                .filter_map(|(name, val)| val.as_str().map(|s| (name.clone(), s.to_string())))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    Ok(dependencies)
}

fn generate_lockfile(packages: &[ResolvedPackage]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let package_json_data = std::fs::read_to_string("package.json")?;
    let package_json: serde_json::Value = serde_json::from_str(&package_json_data)?;

    let root_name = package_json
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or("rnp-project")
        .to_string();

    let root_version = package_json
        .get("version")
        .and_then(|value| value.as_str())
        .unwrap_or("1.0.0")
        .to_string();

    let root_dependencies = package_json
        .get("dependencies")
        .and_then(|v| v.as_object())
        .map(|deps| {
            deps.iter()
                .filter_map(|(name, val)| val.as_str().map(|s| (name.clone(), s.to_string())))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    let workspace_paths = load_workspace_packages()?
        .into_iter()
        .map(|(name, pkg)| (name, pkg.path.to_string_lossy().to_string()))
        .collect::<BTreeMap<_, _>>();

    let mut lock_packages = BTreeMap::new();
    lock_packages.insert(
        "".to_string(),
        LockfilePackage {
            version: root_version.clone(),
            resolved: String::new(),
            integrity: None,
            dependencies: root_dependencies.clone(),
            shasum: None,
        },
    );

    for package in packages {
        let dependencies = package
            .info
            .dependencies
            .iter()
            .map(|(name, req)| (name.clone(), req.display()))
            .chain(
                package
                    .info
                    .peer_dependencies
                    .iter()
                    .map(|(name, req)| (name.clone(), req.display())),
            )
            .chain(
                package
                    .info
                    .optional_dependencies
                    .iter()
                    .map(|(name, req)| (name.clone(), req.display())),
            )
            .collect::<BTreeMap<_, _>>();

        let lock_path = format!("node_modules/{}", package.info.name);
        lock_packages.insert(
            lock_path,
            LockfilePackage {
                version: package.info.version.to_string(),
                resolved: package.info.tarball_url.clone(),
                integrity: package.info.integrity.clone(),
                dependencies,
                shasum: package.info.shasum.clone(),
            },
        );
    }

    let lockfile = PackageLock {
        name: root_name,
        version: root_version,
        lockfile_version: 1,
        requires: true,
        dependencies: root_dependencies,
        workspace_paths,
        packages: lock_packages,
    };

    let serialized = serde_json::to_string_pretty(&lockfile)?;
    std::fs::write("package-lock.json", serialized)?;
    Ok(())
}

fn packages_from_lockfile(
    lockfile: &PackageLock,
) -> Result<Vec<ResolvedPackage>, Box<dyn std::error::Error + Send + Sync>> {
    let mut packages = Vec::new();

    for (path_key, locked) in &lockfile.packages {
        let Some(name) = lockfile_package_name(path_key) else {
            continue;
        };

        let version = Version::parse(&locked.version)?;
        let dependencies = locked
            .dependencies
            .iter()
            .filter_map(|(dep, req)| NpmVersionReq::parse(req).ok().map(|r| (dep.clone(), r)))
            .collect::<HashMap<_, _>>();

        let depth = path_key.matches("node_modules/").count();

        let workspace_path = lockfile.workspace_paths.get(name).map(PathBuf::from);
        let is_workspace = workspace_path.is_some();

        let info = PackageInfo {
            name: name.to_string(),
            version,
            dependencies,
            peer_dependencies: HashMap::new(),
            optional_dependencies: HashMap::new(),
            tarball_url: locked.resolved.clone(),
            integrity: locked.integrity.clone(),
            shasum: locked.shasum.clone(),
            is_workspace,
            workspace_path,
            engines_node: None,
            os_constraints: Vec::new(),
            cpu_constraints: Vec::new(),
            lifecycle_scripts: HashMap::new(),
            bin_entries: HashMap::new(),
        };

        packages.push(ResolvedPackage {
            info,
            depth,
            optional: false,
        });
    }

    Ok(packages)
}

fn ensure_lockfile_in_sync(
    lockfile: &PackageLock,
    manifest_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if manifest_path != Path::new("package.json") {
        return Ok(());
    }

    let manifest_deps = read_manifest_dependencies_from(manifest_path)?;
    if manifest_deps != lockfile.dependencies {
        return Err(format!(
            "{} and package-lock.json are out of sync. Run `rnp install` first.",
            manifest_path.display()
        )
        .into());
    }

    Ok(())
}

fn validate_peer_dependencies(packages: &[ResolvedPackage], options: &InstallOptions) {
    let installed_versions: HashMap<&str, &Version> = packages
        .iter()
        .map(|p| (p.info.name.as_str(), &p.info.version))
        .collect();

    for package in packages {
        for (peer_name, peer_req) in &package.info.peer_dependencies {
            match installed_versions.get(peer_name.as_str()) {
                Some(version) if peer_req.matches(version) => {
                    options.debug(&format!(
                        "peer dependency satisfied: {} -> {}@{}",
                        package.info.name, peer_name, version
                    ));
                }
                Some(version) => {
                    options.warn(&format!(
                        "peer dependency mismatch for {}: expected {} {}, found {}",
                        package.info.name, peer_name, peer_req, version
                    ));
                }
                None => {
                    options.warn(&format!(
                        "missing peer dependency for {}: {} {}",
                        package.info.name, peer_name, peer_req
                    ));
                }
            }
        }
    }
}

fn default_bin_name(package_name: &str) -> String {
    package_name
        .rsplit('/')
        .next()
        .unwrap_or(package_name)
        .to_string()
}

fn current_node_version() -> Option<Version> {
    let output = Command::new("node").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let trimmed = raw.trim().trim_start_matches('v');
    Version::parse(trimmed).ok()
}

fn current_node_os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        "windows" => "win32",
        other => other,
    }
}

fn current_node_cpu() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "x86" => "ia32",
        "aarch64" => "arm64",
        "arm" => "arm",
        other => other,
    }
}

fn verify_integrity_sha512(data: &[u8], integrity: &str) -> bool {
    let Some(encoded) = integrity.strip_prefix("sha512-") else {
        return false;
    };

    let Ok(expected_bytes) = STANDARD.decode(encoded) else {
        return false;
    };

    let mut hasher = Sha512::new();
    hasher.update(data);
    let actual = hasher.finalize();
    actual.as_slice() == expected_bytes.as_slice()
}

fn verify_tarball_integrity(package: &PackageInfo, data: &[u8]) -> Result<(), String> {
    if let Some(integrity) = package.integrity.as_deref() {
        if !verify_integrity_sha512(data, integrity) {
            return Err(format!(
                "integrity verification failed for {}@{}",
                package.name, package.version
            ));
        }
        return Ok(());
    }

    if let Some(expected_shasum) = package.shasum.as_deref()
        && !PackageCache::verify_sha1_checksum(data, expected_shasum)
    {
        return Err(format!(
            "checksum verification failed for {}@{}",
            package.name, package.version
        ));
    }

    Ok(())
}

fn constraint_allows_current(constraints: &[String], current: &str) -> bool {
    if constraints.is_empty() {
        return true;
    }

    let mut positive = Vec::new();
    let mut negative = Vec::new();
    for rule in constraints {
        if let Some(excluded) = rule.strip_prefix('!') {
            negative.push(excluded);
        } else {
            positive.push(rule.as_str());
        }
    }

    if negative.iter().any(|item| *item == current) {
        return false;
    }

    if positive.is_empty() {
        true
    } else {
        positive.iter().any(|item| *item == current)
    }
}

fn validate_package_constraints(
    package: &PackageInfo,
    node_version: Option<&Version>,
) -> Result<(), String> {
    if let Some(node_req) = &package.engines_node {
        if let Some(node_version) = node_version {
            if !node_req.matches(node_version) {
                return Err(format!(
                    "{} requires node '{}', current is {}",
                    package.name,
                    node_req.display(),
                    node_version
                ));
            }
        }
    }

    let os = current_node_os();
    if !constraint_allows_current(&package.os_constraints, os) {
        return Err(format!(
            "{} is not supported on os '{}': {:?}",
            package.name, os, package.os_constraints
        ));
    }

    let cpu = current_node_cpu();
    if !constraint_allows_current(&package.cpu_constraints, cpu) {
        return Err(format!(
            "{} is not supported on cpu '{}': {:?}",
            package.name, cpu, package.cpu_constraints
        ));
    }

    Ok(())
}

fn create_bin_links(
    package: &PackageInfo,
    package_root: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if package.bin_entries.is_empty() {
        return Ok(());
    }

    let bin_dir = Path::new("node_modules").join(".bin");
    fs::create_dir_all(&bin_dir)?;

    for (bin_name, rel_path) in &package.bin_entries {
        let src = package_root.join(rel_path);
        let dst = bin_dir.join(bin_name);

        if dst.exists() {
            fs::remove_file(&dst)?;
        }

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&src, &dst)?;
            if let Ok(metadata) = fs::metadata(&src) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o755);
                let _ = fs::set_permissions(&src, perms);
            }
        }

        #[cfg(windows)]
        {
            let script = format!(
                "@echo off\r\nnode \"%~dp0\\..\\{}\\{}\" %*\r\n",
                package.name,
                rel_path.replace('/', "\\")
            );
            fs::write(dst.with_extension("cmd"), script)?;
        }
    }

    Ok(())
}

fn run_lifecycle_scripts(
    package: &PackageInfo,
    package_root: &Path,
    options: &InstallOptions,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if options.ignore_scripts || package.lifecycle_scripts.is_empty() {
        return Ok(());
    }

    for script_name in ["preinstall", "install", "postinstall"] {
        let Some(script_cmd) = package.lifecycle_scripts.get(script_name) else {
            continue;
        };

        options.debug(&format!("running {} for {}", script_name, package.name));

        #[cfg(unix)]
        let status = Command::new("sh")
            .arg("-c")
            .arg(script_cmd)
            .current_dir(package_root)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;

        #[cfg(windows)]
        let status = Command::new("cmd")
            .arg("/C")
            .arg(script_cmd)
            .current_dir(package_root)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;

        if !status.success() {
            return Err(format!(
                "lifecycle script '{}' failed for {} with status {}",
                script_name, package.name, status
            )
            .into());
        }
    }

    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if ty.is_file() {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn symlink_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(src, dst)
}

#[cfg(windows)]
fn symlink_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(src, dst)
}

fn build_nested_node_modules(packages: &[ResolvedPackage], options: &InstallOptions) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if options.hoist_strategy == "none" {
        return Ok(());
    }

    let root = Path::new("node_modules");
    let all_package_names = packages
        .iter()
        .map(|p| p.info.name.clone())
        .collect::<Vec<_>>();

    for package in packages {
        let package_root = root.join(&package.info.name);
        let nested = package_root.join("node_modules");
        fs::create_dir_all(&nested)?;

        let dependency_names = if options.hoist_strategy == "aggressive" {
            all_package_names.clone()
        } else {
            package
                .info
                .dependencies
                .keys()
                .chain(package.info.peer_dependencies.keys())
                .chain(package.info.optional_dependencies.keys())
                .cloned()
                .collect::<Vec<_>>()
        };

        for dep_name in dependency_names {
            let hoisted_dep = root.join(&dep_name);
            if !hoisted_dep.exists() {
                continue;
            }

            let nested_dep = nested.join(&dep_name);
            if nested_dep.exists() {
                continue;
            }

            if let Err(err) = symlink_dir(&hoisted_dep, &nested_dep) {
                options.warn(&format!(
                    "could not create nested symlink {} -> {} ({})",
                    nested_dep.display(),
                    hoisted_dep.display(),
                    err
                ));
            }
        }
    }

    Ok(())
}

pub async fn handle_ci_command_async(
    options: InstallOptions,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !Path::new("package-lock.json").exists() {
        return Err("package-lock.json not found. `rnp ci` requires a lockfile.".into());
    }

    let workspace_packages = load_workspace_packages()?;
    let manifest_path = workspace_manifest_path(options.workspace.as_deref(), &workspace_packages)?;
    if !manifest_path.exists() {
        return Err(format!("{} not found", manifest_path.display()).into());
    }

    let lock_data = fs::read_to_string("package-lock.json")?;
    let lockfile: PackageLock = serde_json::from_str(&lock_data)?;
    ensure_lockfile_in_sync(&lockfile, &manifest_path)?;

    let packages = packages_from_lockfile(&lockfile)?;
    if packages.is_empty() {
        options.info("Nothing to install from lockfile.");
        return Ok(());
    }

    let resolver = DependencyResolver::new(workspace_packages);
    let node_version = current_node_version();
    let total = resolver
        .install_packages_parallel(&packages, &options, node_version)
        .await?;
    build_nested_node_modules(&packages, &options)?;

    options.success(&format!("Installed {} package(s) from lockfile", total));
    Ok(())
}

// Updated main install function
pub async fn handle_install_command_async(
    package: &str,
    options: InstallOptions,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let workspace_packages = load_workspace_packages()?;
    let manifest_path = workspace_manifest_path(options.workspace.as_deref(), &workspace_packages)?;

    if !manifest_path.exists() {
        options.warn(&format!("{} not found. Please run `rnp init` first.", manifest_path.display()));
        return Ok(());
    }

    options.info(&format!("Resolving dependency tree for {}...", package));

    let mut resolver = DependencyResolver::new(workspace_packages);
    let node_version = current_node_version();
    if node_version.is_none() {
        options.warn("Node.js version could not be detected; engines checks are skipped.");
    }
    let locked_versions = if options.no_package_lock {
        HashMap::new()
    } else {
        load_locked_versions()?
    };

    // Phase 1: Resolve all dependencies
    let packages = resolver
        .resolve_dependencies(package, Some(&locked_versions))
        .await?;

    // Report any conflicts
    if !resolver.conflicts.is_empty() {
        options.warn("Dependency conflicts detected:");
        for conflict in &resolver.conflicts {
            options.warn(conflict);
        }
    }

    options.info(&format!("Found {} package(s) to install", packages.len()));

    // Find the root package (the one user requested, should be at depth 0)
    let root_package = packages
        .iter()
        .find(|p| p.info.name == package && p.depth == 0)
        .ok_or_else(|| format!("Root package '{}' not found in resolved packages", package))?;

    options.info(&format!("Resolved {} to version {}", package, root_package.info.version));

    validate_peer_dependencies(&packages, &options);

    // Phase 2: Install packages in parallel
    let total_installed = resolver
        .install_packages_parallel(&packages, &options, node_version)
        .await?;

    // Phase 3: Build nested node_modules links while keeping hoisted packages at root
    build_nested_node_modules(&packages, &options)?;

    // Phase 4: Update package.json with the ROOT package version
    update_package_json(&manifest_path, package, &root_package.info.version, &options).await?;

    // Phase 5: Generate lockfile unless disabled by flag
    if options.no_package_lock {
        options.debug("Skipping package-lock.json generation (--no-package-lock)");
    } else {
        generate_lockfile(&packages)?;
        options.success("Updated package-lock.json");
    }

    options.success(&format!("Successfully added {} package(s)!", total_installed));
    Ok(())
}

async fn update_package_json(
    package_json_path: &Path,
    package: &str,
    resolved_version: &Version,
    options: &InstallOptions,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Read existing package.json
    let data = std::fs::read_to_string(package_json_path)?;
    let mut json: serde_json::Value = serde_json::from_str(&data)?;

    // Ensure package.json root is a valid JSON object
    let obj = json.as_object_mut().ok_or("Invalid package.json format")?;

    let deps = obj
        .entry("dependencies")
        .or_insert(serde_json::Value::Object(serde_json::Map::new()));

    // Add package with caret range (npm default behavior)
    if let serde_json::Value::Object(map) = deps {
        let version_range = format!("^{}", resolved_version);
        map.insert(
            package.to_string(),
            serde_json::Value::String(version_range),
        );
    }

    // Write back with pretty formatting
    let formatted = serde_json::to_string_pretty(&json)?;
    std::fs::write(package_json_path, formatted)?;

    options.success(&format!("Updated package.json with {}@^{}", package, resolved_version));
    Ok(())
}
