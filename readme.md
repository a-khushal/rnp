# RNP

Rust Node Package Manager CLI (like npm, but built in Rust)

## 🚀 Features

- `rnp init` — Initialize a `package.json` file
- `rnp init --yes` — Initialize with default values (no prompts)
- `rnp install <package>` — Simulated install of a package
- `rnp list` — List installed packages (coming soon)

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
- [ ] **Caching System**
  - [ ] TAR ball caching in `~/.rnp/cache`
  - [ ] Cache invalidation logic
  - [ ] Checksum verification
- [ ] **Lockfile Support**
  - [ ] `package-lock.json` generation
  - [ ] Deterministic installs from lockfile
  - [ ] `--no-package-lock` flag
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
