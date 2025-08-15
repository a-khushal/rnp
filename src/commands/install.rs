use reqwest;
use semver::{Version, VersionReq};
use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tar;
use flate2;

fn parse_npm_version(version_str: &str) -> Result<VersionReq, Box<dyn Error>> {
    let mut version_str = version_str.trim();
    // Handle npm's "||" syntax by trying the first requirement.
    if version_str.contains("||") {
        version_str = version_str.split("||").next().unwrap_or("").trim();
    }
    // Handle hyphen ranges, e.g., "1.2.3 - 2.3.4" -> ">=1.2.3, <=2.3.4"
    if let Some(pos) = version_str.find(" - ") {
        let (start, end) = version_str.split_at(pos);
        let end = &end[3..];
        let formatted_req = format!(">={}, <={}", start.trim(), end.trim());
        return Ok(VersionReq::parse(&formatted_req)?);
    }

    // Handle "x" or "*" wildcards
    if version_str.contains(['x', 'X', '*']) {
        // "1.2.x" becomes "~1.2.0" (>=1.2.0, <1.3.0)
        // "1.x" or "1.*" becomes "^1.0.0" (>=1.0.0, <2.0.0)
        let parts: Vec<&str> = version_str.split('.').collect();
        let operator =
            if parts.len() >= 2 && (parts[1] == "x" || parts[1] == "X" || parts[1] == "*") {
                // Use caret for "1.x" style ranges
                "^"
            } else {
                // Use tilde for "1.2.x" style ranges
                "~"
            };
        let version_str_with_wildcard = version_str.replace(['x', 'X', '*'], "0");
        let formatted_req = format!("{}{}", operator, version_str_with_wildcard);
        return Ok(VersionReq::parse(&formatted_req)?);
    }

    // Replace spaces between version parts with commas, as required by the semver crate.
    // e.g., ">=1.0.0 <2.0.0" -> ">=1.0.0, <2.0.0"
    if version_str.contains(' ') && version_str.contains(['<', '>']) {
        let version_str_with_commas = version_str.replace(" ", ", ");
        if let Ok(req) = VersionReq::parse(&version_str_with_commas) {
            return Ok(req);
        }
    }

    Ok(VersionReq::parse(version_str)?)
}

#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub name: String,
    pub version: Version,
    pub dependencies: HashMap<String, VersionReq>,
    pub tarball_url: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    pub info: PackageInfo,
    pub depth: usize,
}

pub struct DependencyResolver {
    registry_client: Arc<reqwest::Client>,
    conflicts: Vec<String>,
}

impl DependencyResolver {
    pub fn new() -> Self {
        Self {
            registry_client: Arc::new(reqwest::Client::new()),
            conflicts: Vec::new(),
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
        root_package: &str
    ) -> Result<Vec<ResolvedPackage>, Box<dyn std::error::Error + Send + Sync>> { 
        // local variable to store the packages to resolve
        let mut to_resolve: VecDeque<(String, VersionReq, usize)> = VecDeque::new();
        // local variable to store the resolved packages
        let mut resolved: HashMap<String, (Version, usize)> = HashMap::new();
        // local variable to store the resolved packages
        let mut resolved_packages: HashMap<String, ResolvedPackage> = HashMap::new();
        
        // push the root package to the to_resolve queue
        to_resolve.push_back((root_package.to_string(), VersionReq::parse("*")?, 0));

        // classic BFS
        while let Some((package_name, version_req, depth)) = to_resolve.pop_front() {
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
                        package_name, version_req, existing_version_req
                    ));
                    continue;
                }
            }

            // fetch the package metadata
            let package_info = self.fetch_package_metadata(&package_name, &version_req).await?;

            // insert the package into the resolved map
            resolved.insert(package_name.clone(), (package_info.version.clone(), depth));

            // insert the package into the resolved packages map which is global
            resolved_packages.insert(
                package_name.clone(),
                ResolvedPackage {
                    info: package_info.clone(),
                    depth,
                },
            );

            // push the dependencies to the to_resolve queue
            for (dep_name, dep_version_req) in &package_info.dependencies {
                to_resolve.push_back((dep_name.clone(), dep_version_req.clone(), depth + 1));
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
        version_req: &VersionReq,
    ) -> Result<PackageInfo, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("https://registry.npmjs.org/{}", name);
        let response = self.registry_client.get(&url).send().await?;
        let metadata: serde_json::Value = response.json().await?;

        // Find best matching version
        let versions = metadata["versions"]
            .as_object()
            .ok_or("No versions found")?;

        let best_version = self.find_best_version(versions.keys(), version_req)?;
        let version_info = &metadata["versions"][&best_version.to_string()];

        // Parse dependencies
        let mut dependencies = HashMap::new();
        if let Some(deps) = version_info.get("dependencies") {
            if let Some(deps_obj) = deps.as_object() {
                for (dep_name, dep_version) in deps_obj {
                    if let Some(version_str) = dep_version.as_str() {
                        match parse_npm_version(version_str) {
                            Ok(req) => {
                                dependencies.insert(dep_name.clone(), req);
                            }
                            Err(e) => {
                                println!(
                                    "⚠️  Warning: Could not parse version requirement for '{}': '{}'. Error: {}. Using '*' as fallback.",
                                    dep_name, version_str, e
                                );
                                if let Ok(any_version_req) = VersionReq::parse("*") {
                                    dependencies.insert(dep_name.clone(), any_version_req);
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

        Ok(PackageInfo {
            name: name.to_string(),
            version: best_version,
            dependencies,
            tarball_url,
        })
    }

    fn find_best_version(
        &self,
        available_versions: serde_json::map::Keys,
        requirement: &VersionReq,
    ) -> Result<Version, Box<dyn std::error::Error + Send + Sync>> {
        let mut matching_versions: Vec<Version> = available_versions
            .filter_map(|v| Version::parse(v).ok())
            .filter(|v| requirement.matches(v))
            .collect();

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
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        const MAX_CONCURRENT_DOWNLOADS: usize = 15;
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_DOWNLOADS));

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
                let package_info = package.info.clone(); // ← Clone BEFORE async move

                let handle = tokio::spawn(async move {
                    let _permit = semaphore.acquire().await.unwrap();
                    Self::download_and_extract_package(client, &package_info).await
                });

                depth_handles.push(handle);
            }

            // Wait for all packages at this depth to complete
            for handle in depth_handles {
                if handle.await?? {
                    total_installed += 1;
                }
            }
        }

        Ok(total_installed)
    }

    async fn download_and_extract_package(
        client: Arc<reqwest::Client>,
        package: &PackageInfo,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        use std::fs;
        use std::path::Path;

        // Download tarball
        let response = client.get(&package.tarball_url).send().await?;
        let bytes = response.bytes().await?;

        // Extract to node_modules
        let node_modules_path = Path::new("node_modules").join(&package.name);
        fs::create_dir_all(&node_modules_path)?;

        // Extract tarball (similar to your existing code but with async/await)
        let tar = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes));
        let mut archive = tar::Archive::new(tar);

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_owned();

            let mut components = path.components();
            components.next(); // Skip top-level folder
            let relative_path = components.as_path();
            let dest_path = node_modules_path.join(relative_path);

            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }

            entry.unpack(dest_path)?;
        }

        Ok(true)
    }
}

// Updated main install function
pub async fn handle_install_command_async(
    package: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let path = "package.json";

    if !std::path::Path::new(path).exists() {
        println!("❌ Error: package.json not found. Please run `rnp init` first.");
        return Ok(());
    }

    println!("🔍 Resolving dependency tree for {}...", package);

    let mut resolver = DependencyResolver::new();

    // Phase 1: Resolve all dependencies
    let packages = resolver.resolve_dependencies(package).await?;

    // Report any conflicts
    if !resolver.conflicts.is_empty() {
        println!("⚠️  Dependency conflicts detected:");
        for conflict in &resolver.conflicts {
            println!("{}", conflict);
        }
    }

    println!("Found {} packages to install", packages.len());

    // Find the root package (the one user requested, should be at depth 0)
    let root_package = packages
        .iter()
        .find(|p| p.info.name == package && p.depth == 0)
        .ok_or_else(|| format!("Root package '{}' not found in resolved packages", package))?;

    println!(
        "📝 Resolved {} to version {}",
        package, root_package.info.version
    );

    // Phase 2: Install packages in parallel
    let total_installed = resolver.install_packages_parallel(&packages).await?;

    // Phase 3: Update package.json with the ROOT package version
    update_package_json(package, &root_package.info.version).await?;

    println!("🎉 Successfully added {} packages!", total_installed);
    Ok(())
}

async fn update_package_json(
    package: &str,
    resolved_version: &Version,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Read existing package.json
    let data = std::fs::read_to_string("package.json")?;
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
    std::fs::write("package.json", formatted)?;

    println!(
        "Updated package.json with {}@^{}",
        package, resolved_version
    );
    Ok(())
}
