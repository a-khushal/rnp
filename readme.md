# RNP (Rust Node Package Manager)

A fast, reliable package manager for Node.js, built with Rust. RNP provides npm-like functionality with improved performance and reliability.

## 🚀 Features

- `rnp init` — Initialize a `package.json` file
- `rnp init --yes` — Initialize with default values (no prompts)
- `rnp install <package>` — Simulated install of a package
- `rnp install <package> --no-package-lock` — Install without reading/writing lockfile
- `rnp install <package> --ignore-scripts` — Skip lifecycle scripts
- `rnp install -w <workspace> <package>` — Add dependency to a workspace package
- `rnp install --hoist <none|safe|aggressive> <package>` — Control hoist strategy
- `rnp install --verbose <package>` — Detailed logs
- `rnp install --quiet <package>` — Minimal output
- `rnp uninstall <package...>` — Remove package(s)
- `rnp update [package...]` — Update one, many, or all dependencies
- `rnp ci` — Strict lockfile-only deterministic install
- `rnp run <script> [args...]` — Run package scripts
- `rnp audit` — Run security audit against npm advisories
- `rnp list` — List installed packages (coming soon)
- `~/.rnp/cache` — Automatic tarball caching for faster installs
- `package-lock.json` — Generated lockfile for deterministic installs
- Progress bars and colorized output for install flow
- Workspace-aware installs (basic monorepo support)

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
./target/release/rnp i <package-name> <another-package-name>
./target/release/rnp install <package-name> --no-package-lock
./target/release/rnp install <package-name> --ignore-scripts
./target/release/rnp install -w <workspace-name> <package-name>
./target/release/rnp install --hoist aggressive <package-name>
./target/release/rnp install --verbose <package-name>
./target/release/rnp install --quiet <package-name>
./target/release/rnp uninstall <package-name>
./target/release/rnp update
./target/release/rnp update <package-name>
./target/release/rnp ci
./target/release/rnp ci -w <workspace-name>
./target/release/rnp run test
./target/release/rnp run build -- --watch
./target/release/rnp audit
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
- [x] **Node Modules**
  - [x] Nested `node_modules` structure
  - [x] Peer dependencies support
  - [x] Dependency hoisting

### Medium Priority
- [x] **CLI Improvements**
  - [x] Progress bars
  - [x] Colorized output
  - [x] `--verbose` and `--quiet` flags
- [x] **Dependency Resolution**
  - [x] Peer dependencies
  - [x] Optional dependencies
  - [x] Workspaces/monorepo support

### Future Features
- [x] `rnp uninstall` - Remove packages
- [x] `rnp update` - Update packages
- [x] `rnp run` - Run package scripts
- [x] `rnp audit` - Security audits

### Next Milestones
- [x] **Install Correctness & npm Parity**
  - [x] Respect `engines` field and warn/error for incompatible Node versions
  - [x] Handle `os` and `cpu` constraints during install
  - [x] Support lifecycle scripts (`preinstall`, `install`, `postinstall`) with opt-out flag
  - [x] Better semver/range compatibility for complex npm ranges
  - [x] Preserve/install package bin links into `node_modules/.bin`
- [x] **Lockfile & Reproducibility**
  - [x] Add lockfile integrity field verification (`integrity`, sha512)
  - [x] Save dependency tree paths (closer to npm lockfile format)
  - [x] Add `rnp ci` for strict lockfile-only, deterministic installs
  - [x] Fail install when lockfile and manifest are out of sync (in CI mode)
- [ ] **Workspaces (Advanced)**
  - [x] Workspace-aware install filtering (`-w/--workspace`)
  - [x] Hoist strategy config (`none`, `safe`, `aggressive`)
  - [ ] Cross-workspace linking and script execution ordering
  - [x] Workspace-focused lockfile metadata
- [ ] **Dependency Management UX**
  - [ ] Add `rnp add` alias and `-D/--save-dev`, `-O/--save-optional`, `--save-peer`
  - [ ] Add `rnp remove` alias for uninstall parity
  - [ ] Add `rnp outdated` to compare installed vs latest versions
  - [ ] Add `rnp why <package>` to explain dependency origin
- [ ] **Security & Supply Chain**
  - [ ] Add `rnp audit fix` with safe/force modes
  - [ ] Verify tarball signatures where available
  - [ ] Add allow/deny policy for registries and package scopes
  - [ ] Add minimal SBOM export (`cyclonedx`/`spdx`)
- [ ] **CLI/Developer Experience**
  - [ ] Better error formatting with actionable hints
  - [ ] JSON output mode for machine-readable logs (`--json`)
  - [ ] Shell completion generation (bash/zsh/fish/powershell)
  - [ ] Config file support (`.rnprc`, project + global)
- [ ] **Performance & Reliability**
  - [ ] Smarter parallel extraction scheduling by package size
  - [ ] Retry/backoff and mirror fallback for registry fetches
  - [ ] Offline mode (`--offline`) using cache-only installs
  - [ ] Install benchmarks and regression performance tests
- [ ] **Testing & Quality Gates**
  - [ ] Integration test suite with fixture projects
  - [ ] Golden snapshot tests for lockfile generation
  - [ ] Cross-platform filesystem behavior tests (linux/macOS/windows)
  - [ ] GitHub Actions CI matrix with lint, build, and tests

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## 📝 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
