# RNP

Rust Node Package Manager CLI (like npm, but built in Rust)

## Features

- `rnp init` — Initialize a `package.json` file
- `rnp init --yes` — Initialize with default values (no prompts)
- `rnp install <package>` — Simulated install of a package
- `rnp list` — List installed packages (coming soon)

## Installation

```bash
git clone https://github.com/your-username/rnp.git
cd rnp
cargo build --release

## Usage
./target/release/rnp init or ./target/release/rnp init -y
./target/release/rnp install <package-name>
./target/release/rnp list
