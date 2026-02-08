use serde_json::Value;
use std::error::Error;
use std::fs;
use std::path::Path;

pub fn handle_uninstall_command(
    packages: &[String],
    quiet: bool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if !Path::new("package.json").exists() {
        if !quiet {
            eprintln!("package.json not found. Please run `rnp init` first.");
        }
        return Ok(());
    }

    let package_json_data = fs::read_to_string("package.json")?;
    let mut package_json: Value = serde_json::from_str(&package_json_data)?;

    let mut removed_from_manifest = 0usize;
    if let Some(root) = package_json.as_object_mut() {
        for field in [
            "dependencies",
            "devDependencies",
            "peerDependencies",
            "optionalDependencies",
        ] {
            if let Some(Value::Object(dep_map)) = root.get_mut(field) {
                for package in packages {
                    if dep_map.remove(package).is_some() {
                        removed_from_manifest += 1;
                    }
                }
            }
        }
    }

    fs::write("package.json", serde_json::to_string_pretty(&package_json)?)?;

    let mut removed_from_node_modules = 0usize;
    for package in packages {
        let path = Path::new("node_modules").join(package);
        if !path.exists() {
            continue;
        }

        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || metadata.is_file() {
            fs::remove_file(&path)?;
        } else {
            fs::remove_dir_all(&path)?;
        }
        removed_from_node_modules += 1;
    }

    if Path::new("package-lock.json").exists() {
        let lock_data = fs::read_to_string("package-lock.json")?;
        let mut lock_json: Value = serde_json::from_str(&lock_data)?;
        if let Some(packages_obj) = lock_json
            .get_mut("packages")
            .and_then(|v| v.as_object_mut())
        {
            for package in packages {
                packages_obj.remove(package);
            }
        }
        fs::write(
            "package-lock.json",
            serde_json::to_string_pretty(&lock_json)?,
        )?;
    }

    if !quiet {
        println!(
            "Removed {} package entries from package.json and {} folder(s) from node_modules.",
            removed_from_manifest, removed_from_node_modules
        );
    }

    Ok(())
}
