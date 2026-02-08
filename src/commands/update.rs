use crate::commands::install::{InstallOptions, handle_install_command_async};
use serde_json::Value;
use std::error::Error;
use std::fs;
use std::path::Path;

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
        read_dependencies_from_manifest()? 
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
        handle_install_command_async(&package, options).await?;
    }

    Ok(())
}

fn read_dependencies_from_manifest() -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
    let data = fs::read_to_string("package.json")?;
    let json: Value = serde_json::from_str(&data)?;

    let mut deps = Vec::new();
    if let Some(obj) = json.get("dependencies").and_then(|v| v.as_object()) {
        for key in obj.keys() {
            deps.push(key.clone());
        }
    }

    Ok(deps)
}
