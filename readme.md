# RNP (Rust Node Package Manager)

A fast, reliable package manager for Node.js, built with Rust. RNP provides npm-like functionality with improved performance and reliability.

## 🚀 Features

- `rnp init` — Initialize a `package.json` file
- `rnp init --yes` — Initialize with default values (no prompts)
- `rnp install <package>` — Simulated install of a package
- `rnp install <package> --no-package-lock` — Install without reading/writing lockfile
- `rnp list` — List installed packages (coming soon)
- `~/.rnp/cache` — Automatic tarball caching for faster installs
- `package-lock.json` — Generated lockfile for deterministic installs

## Installation

```bash
git clone https://github.com/a-khushal/rnp.git
cd rnp
cargo build --release
```

## Usage

### Basic Usage
```bash
./target/release/rnp init
./target/release/rnp init -y
./target/release/rnp install <package-name>
./target/release/rnp install <package-name> --no-package-lock
./target/release/rnp list
```

### Set up an alias (recommended)
To make `rnp` available anywhere in your terminal, add this to your shell configuration file (`~/.bashrc`, `~/.zshrc`, or `~/.config/fish/config.fish`):

For Bash/Zsh:
```bash
alias rnp='$HOME/<pwd>/rnp/target/release/rnp'
```

For Fish:
```fish
alias rnp='$HOME/<pwd>/rnp/target/release/rnp'
```

Then reload your shell or run:
```bash
source ~/.bashrc  # or ~/.zshrc
```

After setting up the alias, you can use `rnp` directly:
```bash
rnp init
rnp install <package-name>
```

## 🛠️ Roadmap & TODOs

### High Priority
- [x] Basic package installation
- [x] **Caching System**
  - [x] TAR ball caching in `~/.rnp/cache`
  - [x] Cache invalidation logic
  - [x] Checksum verification
- [x] **Lockfile Support**
  - [x] `package-lock.json` generation
  - [x] Deterministic installs from lockfile
  - [x] `--no-package-lock` flag
- [ ] **Node Modules**
  - [ ] Nested `node_modules` structure
  - [ ] Peer dependencies support
  - [ ] Dependency hoisting

### Medium Priority
- [ ] **CLI Improvements**
  - [ ] Progress bars
  - [ ] Colorized output
  - [ ] `--verbose` and `--quiet` flags
- [ ] **Dependency Resolution**
  - [ ] Peer dependencies
  - [ ] Optional dependencies
  - [ ] Workspaces/monorepo support

### Future Features
- [ ] `rnp uninstall` - Remove packages
- [ ] `rnp update` - Update packages
- [ ] `rnp run` - Run package scripts
- [ ] `rnp audit` - Security audits

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## 📝 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
