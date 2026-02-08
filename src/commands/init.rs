use serde::Serialize;
use serde_json::{Map, Value};
use std::env;
use std::fs::File;
use std::io;
use std::io::Write;

#[derive(Serialize, Debug)]
struct Repository {
    #[serde(rename = "type")]
    repo_type: String,
    url: String,
}

#[derive(Serialize, Debug)]
struct PackageJson {
    name: String,
    version: String,
    description: String,
    #[serde(rename = "entry point")]
    main: String,
    scripts: Map<String, serde_json::Value>,
    keywords: Vec<String>,
    author: String,
    license: String,
    #[serde(rename = "type")]
    type_field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    repository: Option<Repository>,
}

fn prompt(field: &str, default: &str) -> String {
    if default.is_empty() {
        print!("{}: ", field);
    } else {
        print!("{}: ({}) ", field, default);
    }

    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let trimmed = input.trim();

    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn handle_init(yes: bool) {
    let current_dir = env::current_dir().unwrap();
    let folder_name = current_dir
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let mut default_scripts = Map::new();
    default_scripts.insert(
        "test".to_string(),
        Value::String("echo \"Error: no test specified\" && exit 1".to_string()),
    );

    let pkg: PackageJson = if yes {
        PackageJson {
            name: folder_name,
            version: "1.0.0".to_string(),
            description: "".to_string(),
            main: "index.js".to_string(),
            scripts: default_scripts,
            keywords: vec![],
            author: "".to_string(),
            license: "ISC".to_string(),
            type_field: "commonjs".to_string(),
            repository: None,
        }
    } else {
        let name = prompt("package name", &folder_name);
        let version = prompt("version", "1.0.0");
        let description = prompt("description", "");
        let main = prompt("entry point", "index.js");
        let git_url = prompt("git repository", "");
        let test_command = prompt("test command", "");
        let keywords_input = prompt("keywords", "");
        let author = prompt("author", "");
        let license = prompt("license", "ISC");
        let type_field = prompt("type", "commonjs");

        let keywords = if !keywords_input.is_empty() {
            keywords_input
                .split_whitespace()
                .map(|s| s.to_string())
                .collect()
        } else {
            vec![]
        };

        let repository = if git_url.is_empty() {
            None
        } else {
            Some(Repository {
                repo_type: "git".to_string(),
                url: git_url,
            })
        };

        let scripts = if test_command.is_empty() {
            default_scripts
        } else {
            let mut scripts = Map::new();
            scripts.insert("test".to_string(), Value::String(test_command));
            scripts
        };

        PackageJson {
            name,
            version,
            description,
            keywords,
            license,
            author,
            type_field,
            main,
            scripts,
            repository,
        }
    };

    let json = serde_json::to_string_pretty(&pkg).unwrap();

    let mut file = File::create("package.json").unwrap();
    file.write_all(json.as_bytes()).unwrap();

    println!("initialized package.json to {}\n", current_dir.display());
    println!("{}\n", json);
}
