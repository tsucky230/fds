# FDS - Lightweight TUI File Manager

A fast, keyboard-driven file manager for terminal environments. Recreates the "responsiveness that lets you act before you think" of classic file managers, rebuilt for Unicode, SSH, and modern terminals.

## ✨ Key Features

- **Keyboard-first**: All operations via keyboard with function key alternatives
- **SSH-ready**: No X11/Wayland needed—runs on remote servers
- **Universal Viewer**: Text, Markdown, hex, images with auto charset detection
- **Archive Support**: ZIP, tar, tar.gz without extra tools
- **Multi-language**: English & Japanese built-in
- **Terminal-safe**: Restores terminal state on exit, crash, or disconnect

## 🚀 Quick Start

### Installation

Requires Rust (stable). At the repository root:

```bash
cargo install --path .
```

Or for Linux/WSL without Rust:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
source ~/.cargo/env
cargo install --path <repository-path> --locked
```

### Launch

```bash
fds           # current directory
fds [path]    # specific directory
fds --help    # usage & key list
```

## 📖 Documentation

- **[Full README](https://github.com/tsucky230/fds)** — Complete features, configuration, keys
- **[Installation Guide](https://github.com/tsucky230/fds#installation)** — Setup steps
- **[Key List](https://github.com/tsucky230/fds#key-list)** — Keyboard shortcuts
- **[Configuration](https://github.com/tsucky230/fds#configuration)** — Settings & customization

## 🤝 Community

- 💬 [GitHub Discussions](https://github.com/tsucky230/fds/discussions)
- 🐛 [Report Issues](https://github.com/tsucky230/fds/issues)
- ☕ [GitHub Sponsors](https://github.com/sponsors/tsucky230)

## 📋 Current Status

**Phase 4: Supporter Infrastructure**

Implemented: Copy, Move, Delete (with trash), Rename, New Folder, Search, Archive (ZIP/tar), Preview

### Supported Platforms

- **OS**: Windows, macOS, Linux
- **Environments**: Local terminal, SSH, tmux, headless
- **Rust**: Stable (tested on 1.95+)

## 📄 License

GNU General Public License v3 only (`GPL-3.0-only`)

- [Full License Text](https://github.com/tsucky230/fds/blob/main/LICENSE)
- [Dependency Attribution](https://github.com/tsucky230/fds/blob/main/NOTICE.md) (314 dependencies)
- [Software Bill of Materials](https://github.com/tsucky230/fds/blob/main/sbom/fds.cdx.json) (CycloneDX)

**All 314 external dependencies are GPL-3.0 compatible** ✓

---

**Learn more**: [View on GitHub](https://github.com/tsucky230/fds)
