use crate::commands::install::{InstallOptions, handle_install_command_async};
use serde_json::Value;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

pub async fn handle_update_command_async(
    packages: Vec<String>,
    options: InstallOptions,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if !Path::new("package.json").exists() {
        if !options.quiet {
            eprintln!("package.json not found. Please run `rnp init` first.");
        }
        return Ok(());
    }

    let targets = if packages.is_empty() {
        let manifest_path = workspace_manifest_path(options.workspace.as_deref())?;
        read_dependencies_from_manifest(&manifest_path)?
    } else {
        packages
    };

    if targets.is_empty() {
        if !options.quiet {
            println!("No dependencies found to update.");
        }
        return Ok(());
    }

    for package in targets {
        handle_install_command_async(&package, options.clone()).await?;
    }

    Ok(())
}

fn read_dependencies_from_manifest(path: &Path) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
    let data = fs::read_to_string(path)?;
    let json: Value = serde_json::from_str(&data)?;

    let mut deps = Vec::new();
    if let Some(obj) = json.get("dependencies").and_then(|v| v.as_object()) {
        for key in obj.keys() {
            deps.push(key.clone());
        }
    }

    Ok(deps)
}

fn workspace_manifest_path(workspace_name: Option<&str>) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    if workspace_name.is_none() {
        return Ok(PathBuf::from("package.json"));
    }

    let workspace_name = workspace_name.unwrap();
    let root_data = fs::read_to_string("package.json")?;
    let root_json: Value = serde_json::from_str(&root_data)?;
    let mut patterns = Vec::new();

    if let Some(workspaces) = root_json.get("workspaces") {
        if let Some(arr) = workspaces.as_array() {
            for item in arr {
                if let Some(pattern) = item.as_str() {
                    patterns.push(pattern.to_string());
                }
            }
        } else if let Some(arr) = workspaces.get("packages").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(pattern) = item.as_str() {
                    patterns.push(pattern.to_string());
                }
            }
        }
    }

    for pattern in patterns {
        let dirs = if let Some(prefix) = pattern.strip_suffix("/*") {
            let mut out = Vec::new();
            for entry in fs::read_dir(prefix)? {
                let entry = entry?;
                if entry.path().is_dir() {
                    out.push(entry.path());
                }
            }
            out
        } else {
            vec![PathBuf::from(pattern)]
        };

        for dir in dirs {
            let pkg_path = dir.join("package.json");
            if !pkg_path.exists() {
                continue;
            }
            let data = fs::read_to_string(&pkg_path)?;
            let json: Value = serde_json::from_str(&data)?;
            if json.get("name").and_then(|v| v.as_str()) == Some(workspace_name) {
                return Ok(pkg_path);
            }
        }
    }

    Err(format!("Workspace '{}' not found", workspace_name).into())
}
