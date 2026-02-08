use reqwest;
use semver::Version;
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;

pub async fn handle_audit_command_async() -> Result<(), Box<dyn Error + Send + Sync>> {
    let installed = load_installed_versions()?;
    if installed.is_empty() {
        println!("No installed dependencies found to audit.");
        return Ok(());
    }

    let payload: HashMap<String, Vec<String>> = installed
        .iter()
        .map(|(name, version)| (name.clone(), vec![version.clone()]))
        .collect();

    let client = reqwest::Client::new();
    let response = client
        .post("https://registry.npmjs.org/-/npm/v1/security/advisories/bulk")
        .json(&payload)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("Audit request failed: {}", response.status()).into());
    }

    let advisories: Value = response.json().await?;
    let Some(map) = advisories.as_object() else {
        println!("No advisories found.");
        return Ok(());
    };

    if map.is_empty() {
        println!("No known vulnerabilities found.");
        return Ok(());
    }

    let mut total = 0usize;
    let mut critical = 0usize;
    let mut high = 0usize;
    let mut moderate = 0usize;
    let mut low = 0usize;

    println!("Security advisories detected:\n");
    for (pkg, entries) in map {
        if let Some(list) = entries.as_array() {
            for advisory in list {
                total += 1;
                let title = advisory
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown advisory");
                let severity = advisory
                    .get("severity")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let url = advisory
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("N/A");

                match severity {
                    "critical" => critical += 1,
                    "high" => high += 1,
                    "moderate" => moderate += 1,
                    "low" => low += 1,
                    _ => {}
                }

                println!("- {} [{}]", pkg, severity);
                println!("  {}", title);
                println!("  {}", url);
            }
        }
    }

    println!("\nSummary:");
    println!("- total: {}", total);
    println!("- critical: {}", critical);
    println!("- high: {}", high);
    println!("- moderate: {}", moderate);
    println!("- low: {}", low);

    Ok(())
}

fn load_installed_versions() -> Result<HashMap<String, String>, Box<dyn Error + Send + Sync>> {
    if Path::new("package-lock.json").exists() {
        return load_versions_from_lockfile();
    }

    if Path::new("package.json").exists() {
        return load_versions_from_manifest();
    }

    Ok(HashMap::new())
}

fn load_versions_from_lockfile() -> Result<HashMap<String, String>, Box<dyn Error + Send + Sync>> {
    let data = fs::read_to_string("package-lock.json")?;
    let json: Value = serde_json::from_str(&data)?;
    let mut versions = HashMap::new();

    if let Some(packages) = json.get("packages").and_then(|v| v.as_object()) {
        for (name, info) in packages {
            let Some(version) = info.get("version").and_then(|v| v.as_str()) else {
                continue;
            };
            if Version::parse(version).is_ok() {
                versions.insert(name.clone(), version.to_string());
            }
        }
    }

    Ok(versions)
}

fn load_versions_from_manifest() -> Result<HashMap<String, String>, Box<dyn Error + Send + Sync>> {
    let data = fs::read_to_string("package.json")?;
    let json: Value = serde_json::from_str(&data)?;
    let mut versions = HashMap::new();

    if let Some(deps) = json.get("dependencies").and_then(|v| v.as_object()) {
        for (name, req) in deps {
            let Some(raw) = req.as_str() else {
                continue;
            };
            let cleaned = raw.trim_start_matches(['^', '~', '>', '<', '=']);
            if Version::parse(cleaned).is_ok() {
                versions.insert(name.clone(), cleaned.to_string());
            }
        }
    }

    Ok(versions)
}
