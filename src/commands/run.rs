use serde_json::Value;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn handle_run_command(
    script_name: &str,
    args: &[String],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if !Path::new("package.json").exists() {
        return Err("package.json not found. Please run `rnp init` first.".into());
    }

    let data = fs::read_to_string("package.json")?;
    let package_json: Value = serde_json::from_str(&data)?;

    let scripts = package_json
        .get("scripts")
        .and_then(|v| v.as_object())
        .ok_or("No scripts section found in package.json")?;

    let script_cmd = scripts
        .get(script_name)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Script '{}' not found in package.json", script_name))?;

    let full_cmd = if args.is_empty() {
        script_cmd.to_string()
    } else {
        format!("{} {}", script_cmd, args.join(" "))
    };

    println!("Running script '{}': {}", script_name, full_cmd);

    #[cfg(unix)]
    let status = Command::new("sh")
        .arg("-c")
        .arg(&full_cmd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    #[cfg(windows)]
    let status = Command::new("cmd")
        .arg("/C")
        .arg(&full_cmd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if !status.success() {
        return Err(format!("Script '{}' failed with status {}", script_name, status).into());
    }

    Ok(())
}
