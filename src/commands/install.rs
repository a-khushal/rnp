use std::fs::{self, File};
use std::io::{Write, Cursor};
use std::path::Path;
use serde_json::{Value, Map};
use reqwest::blocking::get;
use flate2::read::GzDecoder;
use tar::Archive;

fn handle_install(package: &str, verbose: bool) {
    let metadata_url = format!("https://registry.npmjs.org/{}", package);
    let res = get(&metadata_url).expect("Failed to fetch package metadata");
    let metadata: Value = res.json().expect("Failed to parse metadata");

    let latest_version = metadata["dist-tags"]["latest"]
        .as_str()
        .expect("Failed to get latest version");

    let tarball_url = metadata["versions"][latest_version]["dist"]["tarball"]
        .as_str()
        .expect("Failed to get tarball url");

    if verbose {
        println!("Downloading {}@{}", package, latest_version);
    }

    let tarball_bytes = get(tarball_url)
        .expect("Failed to download tarball")
        .bytes()
        .expect("Failed to read tarball bytes");

    let tar = GzDecoder::new(Cursor::new(tarball_bytes));
    let mut archive = Archive::new(tar);

    let node_modules_path = Path::new("node_modules").join(package);
    fs::create_dir_all(&node_modules_path).unwrap();

    for entry in archive.entries().expect("Failed to read tar entries") {
        let mut entry = entry.expect("Failed to get entry");
        let path = entry.path().expect("Failed to get path").to_owned();

        let mut components = path.components();
        components.next(); // skip top-level folder (package/)

        let relative_path = components.as_path();
        let dest_path = node_modules_path.join(relative_path);

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }

        entry.unpack(dest_path).expect("Failed to unpack entry");
    }
}

fn install_recursive(package: &str, is_root: bool) -> usize {
    let mut count = 1;

    handle_install(package, is_root);

    let pkg_json_path = format!("node_modules/{}/package.json", package);
    let pkg_data = fs::read_to_string(&pkg_json_path).expect("Failed to read installed package.json");
    let pkg_json: Value = serde_json::from_str(&pkg_data).expect("Invalid JSON in package.json");

    if let Some(deps) = pkg_json.get("dependencies") {
        if let Some(deps_obj) = deps.as_object() {
            for (dep_name, _) in deps_obj {
                count += install_recursive(dep_name, false);
            }
        }
    }

    count
}

pub fn handle_install_command(package: &str) {
    let path = "package.json";

    if !Path::new(path).exists() {
        println!("Error: package.json not found. Please run `rnp init` first.");
        return;
    }

    let data = fs::read_to_string(path).expect("Unable to read package.json");
    let mut json: Value = serde_json::from_str(&data).expect("Invalid JSON in package.json");

    let total_installed = install_recursive(package, true);

    let metadata_url = format!("https://registry.npmjs.org/{}", package);
    let res = get(&metadata_url).expect("Failed to fetch package metadata");
    let metadata: Value = res.json().expect("Failed to parse metadata");

    let latest_version = metadata["dist-tags"]["latest"]
        .as_str()
        .expect("Failed to get latest version");

    let obj = json.as_object_mut().unwrap();
    let deps = obj.entry("dependencies").or_insert(Value::Object(Map::new()));

    if let Value::Object(map) = deps {
        map.insert(package.to_string(), Value::String(latest_version.to_string()));
    }

    let formatted = serde_json::to_string_pretty(&json).unwrap();
    let mut file = File::create(path).unwrap();
    file.write_all(formatted.as_bytes()).unwrap();

    println!("\nadded {} packages", total_installed);
}
