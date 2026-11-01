# FDS

English | [日本語](README_jp.md) | 🌐 [Website](https://tsucky230.github.io/fds/)

A lightweight, fast TUI file manager focused on keyboard operation.
It recreates the "responsiveness that lets you act before you think" of file managers such as FD, FDclone, FileMente, and Miel, rebuilt for Unicode, SSH, and modern terminal environments (it is not a port of existing products).

Currently, implementation reaches Phase 4 (Supporter Infrastructure). It supports copy, move, delete (including trash), rename, new folder creation, search, editing with external editor, and viewing/creating/extracting ZIP and tar archives. You can add display via Pack (external command). Since version 0.5.0, a preview (text, folder, hex, image) of the cursor entry appears on the right side of the list.

## Features

- **Single-pane first**: There is always exactly one current location on screen
- **Keyboard-first**: All operations are completed via the keyboard. Alternative keys are provided for environments without function keys
- **SSH / headless support**: No X11, Wayland, or desktop environment is required. It runs unchanged on a remote SSH server
- **Universal Viewer**: Press F3 (or Enter on a file) to view text, Markdown, hex, or images (PNG/JPEG). Character encodings such as Shift_JIS are auto-detected
- **File Operations**: Copy / Move / Delete / Rename / new folder / search / external editor. Copy writes to a temporary name then publishes, leaving no corrupted file if it fails. Move deletes the source only after the copy succeeds, even across drives. For name conflicts you can choose overwrite / skip / rename / abort
- **Basic archives**: ZIP / tar / tar.gz / tgz can be browsed, created, and extracted hierarchically without extra tools or a Supporter license
- **Clear UI**: When selecting a destination, a banner at the top shows "what / which operation / destination / relation to source". The whole screen is framed, and a scrollbar appears for long lists
- **Multilingual**: Built-in English and Japanese. Adding or overriding languages is possible by placing language files
- **Terminal-safe**: The terminal state is restored on normal exit, error, panic, signal, or SSH disconnect

## Installation

### Homebrew (macOS / Linux)

```bash
brew install tsucky230/tap/fds
```

The formula in [tsucky230/homebrew-tap](https://github.com/tsucky230/homebrew-tap) builds FDS from the tagged source, so the first install takes a few minutes.

### From source

Rust (stable) is required. Running the following at the repository root installs to `~/.cargo/bin/fds`.

```bash
cargo install --path .
```

If Rust is not installed on Linux / WSL:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
source ~/.cargo/env
cargo install --path <repository path> --locked
```

## Usage

```bash
fds           # launch in the current directory
fds [path]    # launch in the specified directory
fds --version # show version (-V also works)
fds --help    # show usage, key list, and config file location (-h also works)
```

When opening a folder whose name matches an option such as `--version`, prepend `--` like `fds -- --version`. The running version also appears in the heading of the F1 help screen.

### Key List

| Key | Alternative Key | Action |
| --- | --- | --- |
| ↑ / ↓ | k / j | Move cursor |
| PageUp / PageDown | Ctrl+B / Ctrl+F | Move one page |
| Home / End | g / G | To start / end |
| Enter | l | Enter directory |
| Backspace | h / ← | To parent directory |
| Space |  | Toggle mark |
| F1 | ? | Key list |
| Ctrl+R |  | Reload |
| . |  | Toggle hidden files |
| i |  | Toggle info panel (permissions, attributes, size, dates, etc.) |
| P |  | Toggle right preview |
| Tab |  | Move cursor to the list of directory contents (splits right preview vertically, shows contents below. Tab / Esc to return) |
| o |  | Open with OS default app (checks if executable, runs only on y) |
| ! |  | Enter a command to run in the current folder |
| x |  | Enter a command to run on the marks (or the cursor entry if none). On an executable file, add arguments and run it |
| Ctrl+P |  | Toggle preview image display mode in the list |
| F10 | q / Esc | Show exit confirmation (y or Enter to exit, n / Esc to cancel. Ctrl+C exits without confirmation) |
| F3 / Enter | v | View file with Viewer |
| F4 | e | Edit with external editor |
| F5 | c | Copy |
| F6 | m | Move |
| F8 | d | Delete (with confirmation. Default is permanent delete; can be set to move to trash) |
| F2 | r | Rename |
| F7 | n | New folder |
| / |  | Search (move while typing, Esc returns to original position) |
| L | F9 | List of drives, home, mount points (can switch to another drive while selecting destination) |
| p |  | Create ZIP or tar archive from marked items (or the cursor entry if none) |
| u |  | Extract ZIP or tar archive at the cursor entry into a new folder |

On Unix-like systems, press Ctrl+Z to suspend and `fg` to resume.

Copy and move enter "destination selection". Navigate the list and press Enter on the top "Copy / Move here" line to execute (Enter on a folder opens it). Press Esc to cancel.

During execution, the status line shows the count and, when known, the byte size (e.g., `copy 0/1  1.1 GiB / 4.0 GiB`).

Copy writes to a temporary name `.<name>.fds-partial.<pid>-<num>` at the destination, then renames to the final name. On abort or error the temporary file is removed, but it may remain if FDS crashes. If it remains, verify that the process with `<pid>` is not running (Windows: Task Manager Details tab PID; Linux/macOS: `ps -p <pid>`) before deleting. Because a running FDS might be writing, FDS does not delete automatically.

### Executing Commands

Press `!` to open an input line and run a command in the current folder. Pipes and wildcards are supported (Unix uses `/bin/sh`, Windows uses `cmd`). Interactive commands like vim work, and Ctrl+C stops only that command. When it finishes, the result is shown and pressing Enter returns to the list and reloads.

`x` opens the input line with prefilled content.

| Context | Input line | Example |
| --- | --- | --- |
| Marks present | ` {files}` | `tar cf x.tar {files}` |
| On executable file | `./build.sh ` | `./build.sh --release` |
| On other entry | ` {file}` | `cp {file} {file}.bak` |

- `{file}` is the cursor entry, `{files}` is the marks (or the cursor entry if none). A name with spaces or symbols still reaches the command as one argument. They cannot be used inside quotes.
- Use `↑` / `↓` to go through the commands of this session. History is not saved to a file.
- A line consisting only of `cd <path>` changes FDS's location.
- On Windows, `cmd` cannot be given names containing `%` safely, so such a command does not run. Nor do commands in network (UNC) folders.

### Archive Operations

Press Enter or F3 / v on a ZIP, tar, tar.gz, or tgz file to view its contents hierarchically. Use Enter to enter a folder, Backspace to go up, Esc to close. From the view screen you can also extract with `u`. When creating with `p`, the file extension of the name determines the format (e.g., `backup.zip`, `backup.tar`, `backup.tar.gz`).

The extraction destination is a new folder next to the archive. Existing files or folders are not overwritten, and archives containing dangerous paths, links, or special files are rejected. Creation also rejects selected items that are links or special files. Press Esc to abort. If FDS crashes during processing, temporary items starting with `.fds-archive-partial-` or `.fds-extract-partial-` may remain in the same location.

7z and RAR are currently unsupported, planned for future Archive Packs.

### Viewer Keys

| Key | Action |
| --- | --- |
| ↑ ↓ / j k, PageUp PageDown / Ctrl+B Ctrl+F, Home End / g G | Scroll |
| ← → | Horizontal scroll (when line wrap is off) |
| w | Toggle wrap |
| Tab | Show the same file in another Viewer (Markdown → text → hex, etc.) |
| E | Switch character encoding (auto → UTF-8 → Shift_JIS → EUC-JP → ISO-2022-JP → UTF-16LE → UTF-16BE → Windows-1252 → auto). When manually selected, the status line shows "(manual)". |
| p | Toggle image display mode (half-block / Sixel / Kitty / iTerm2) |
| q / Esc / F3 / v / h / Backspace | Return to the list |
| Ctrl+C | Exit FDS |

## Configuration

Configuration file locations:

- Windows: `%APPDATA%\fds\config.toml`
- Linux / macOS: `$XDG_CONFIG_HOME/fds/config.toml` (falls back to `~/.config/fds/config.toml` if unset)

### UI Language

By default it follows the OS language (English if no matching language). It can be fixed in the configuration file.

```toml
[ui]
language = "ja"
```

### Image Display Mode

By default it selects based on the environment (Windows Terminal uses Sixel, Kitty / Ghostty use Kitty, iTerm2 / WezTerm use iTerm2, inside tmux and others use half-block). If unsuitable, you can fix it via configuration.

```toml
[viewer]
image_protocol = "sixel"   # auto / kitty / sixel / iterm2 / halfblocks / none
```

### vi-style Keys for Viewer

Setting `vi_keys = true` in `[viewer]` makes h / l scroll left/right inside the Viewer (h does not close). Backspace, q, Esc, F3, v return to the list. Default is false.

```toml
[viewer]
vi_keys = true
```

### Preview

On terminals at least 80 columns wide, a preview of the cursor entry appears on the right of the list. Text shows its first lines, folders list their contents, binaries show a one-line type and size (Enter shows hex), images (PNG/JPEG) show format and size followed by the image itself. Enabled by default. Toggle with `P`; setting `[ui] preview = false` disables it at startup. Image rendering follows the same `[viewer] image_protocol`; in the list toggle with `Ctrl+P`, in the Viewer with `p`. To stop only the right-hand images, set `preview_images = false` (reverts to showing format and size).

```toml
[ui]
preview = false
preview_images = false
```

### Frame Display

A frame around the whole screen. Enabled by default. Set `[ui] frame = false` to hide. No frame when width < 42 or height < 12.

```toml
[ui]
frame = false
```

### Delete Method

By default F8 performs permanent delete (the confirmation dialog asks "Permanently delete?"). Setting to `trash` moves items to the trash (Windows Recycle Bin, macOS Finder Trash, Linux freedesktop Trash).

```toml
[ops]
delete = "trash"   # permanent (default) / trash
```

- If a location cannot be moved to the trash (e.g., Windows USB drives, subst, network drives, items exceeding the Recycle Bin capacity, Linux filesystems without Trash), the item remains as an error. It will not automatically switch to permanent delete.
- If the value is neither `permanent` nor `trash`, it is treated as a trash setting and reported in the status line at startup.
- If the configuration file is malformed, it falls back to the default (permanent delete). Check the confirmation dialog text to see which behavior applies.

### External Editor

`[editor]` `command` (e.g., `"code -w"`) and per-extension settings `[editor.by_extension]` (e.g., `md = "typora"`). If not specified, `VISUAL` → `EDITOR` → OS default is used.

`[open.by_extension]` lets you specify the app opened with Enter per extension (e.g., `pdf = "zathura"`, `mp4 = { command = "mpv", wait = false }`). The default for `wait` is true, causing FDS to wait until the app finishes. Use `wait = false` for GUI apps. F3 / v always displays inside FDS.

```toml
[editor]
command = "code -w"

[editor.by_extension]
md = "typora"
```

### Adding or Overriding Language Files

Placing `lang/<code>.toml` in the configuration directory adds a language or overrides built-in strings. See the format in [lang/en.toml](lang/en.toml). Unspecified items fall back to English.

## Supporter and Pack

The Core of FDS (all features described in this README) is free to use.

### Supporter License

A Supporter license is required to use official Packs. Placing `supporter.license` in the configuration directory causes an offline verification at startup (no network). The status appears in the Supporter section of the Help screen (F1). The current build does not yet contain the production public key, so no license is currently valid.

### Pack (add display via external command)

In `<config directory>/packs/<folder>/pack.toml` specify extensions and the command to display those files. FDS shows the command's standard output as text. Files with matching extensions open with the Pack display, and Tab switches to the built-in Viewer.

```toml
name = "pdf-text"
version = "1.0.0"

[[provider]]
id = "pdf-text"
label = "PDF (text)"
extensions = ["pdf"]
command = ["pdftotext", "-layout", "{path}", "-"]
```

- Only arguments exactly matching `{path}` are replaced with the file's absolute path. No shell is invoked.
- Standard output is limited to 8 MiB. The default timeout is 10 seconds, configurable via `[providers] timeout_secs` (1–300). Loading can be aborted with q / Esc.
- Official Packs (`official = true`) are loaded only when a signature (`pack.toml.sig`) and a supporting Supporter license are present. Others (community Packs) load without a license.
- Failed Packs and the reasons appear on the Help screen. Packs to disable can be specified by name or folder.

```toml
[packs]
disabled = ["pdf-text"]

[providers]
timeout_secs = 30
```

> [!WARNING]
> Pack commands run with your privileges. Adding a Pack is equivalent to trusting its command. Do not add Packs from unknown sources.

## Runtime Environment

- OS: Windows / macOS / Linux
- Execution environment: local terminal, SSH, tmux, Linux without GUI
- Rust: stable (edition 2024). Tested on 1.95 and 1.98

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

- CI: [.github/workflows/ci.yml](.github/workflows/ci.yml) (Windows / macOS / Linux)

## Support & Contribute

- 📖 **Documentation** — [README](README.md) | 🌐 **Discussions** — [GitHub Discussions](https://github.com/qraft/fds/discussions)
- 🐛 **Report Issues** — [GitHub Issues](https://github.com/tsucky230/fds/issues/new)
- 🤝 **Contribute** — [CONTRIBUTING](CONTRIBUTING.md)
- ☕ **[GitHub Sponsors](https://github.com/sponsors/tsucky230)** / 💖 **Star this project**

## License Compliance

FDS is distributed under [GPL-3.0-only](LICENSE). To comply with the GPL-3.0 requirements, this project includes:

- **[LICENSE](LICENSE)**: Full text of GNU General Public License version 3
- **[NOTICE](NOTICE.md)**: Attribution of all 314 third-party dependencies and their licenses
- **[sbom/fds.cdx.json](sbom/fds.cdx.json)**: Software Bill of Materials (CycloneDX 1.5) with:
  - Complete dependency tree with versions
  - License information for each component
  - Package URLs (purl) and CPE identifiers for vulnerability tracking
  - SHA-256 hashes for integrity verification

**License Audit Results:**
- ✓ All 314 external dependencies are compatible with GPL-3.0-only
- ✓ Licenses include: MIT (68), Apache-2.0 (combined 200+), BSD-3-Clause (3), and others
- ✓ No license conflicts detected
- ✓ "OR" licenses permit GPL-3.0 compatible options (e.g., MIT OR Apache-2.0)

The SBOM is automatically regenerated and committed whenever `Cargo.toml` or `Cargo.lock` changes, ensuring continuous compliance tracking.

## License

Copyright © 2026 Qraft Node Kyoto

FDS is open-source software under the GNU General Public License, version 3 only (`GPL-3.0-only`). See [LICENSE](LICENSE). It comes with absolutely no warranty.

Official Packs are separate programs and have their own terms. The dependencies and their licenses are listed in [sbom/fds.cdx.json](sbom/fds.cdx.json) (CycloneDX), which GitHub Actions keeps up to date.

### Software Bill of Materials (SBOM)

A comprehensive CycloneDX SBOM in JSON format is maintained automatically by GitHub Actions whenever `Cargo.toml` or `Cargo.lock` changes.

**SBOM Details:**
- Format: CycloneDX 1.5 (JSON)
- Location: [`sbom/fds.cdx.json`](sbom/fds.cdx.json)
- Contents:
  - 314+ third-party dependencies with hashes, licenses, and VCS references
  - 6 internal modules (ops, viewer, ui, cli, archive, i18n) with author, license, and component integrity hashes
  - All components include:
    - Package URL (purl) for standardized identification
    - CPE (Common Platform Enumeration) for vulnerability tracking
    - SHA-256 hash for integrity verification
    - VCS reference for traceability

**Verification:**
```bash
# Validate SBOM against CycloneDX schema
curl -L https://raw.githubusercontent.com/CycloneDX/specification/master/schema/bom-1.5.schema.json | \
  python -m json.tool > bom-schema.json
jq --arg sch "$(cat bom-schema.json)" '. as $bom | $sch | {"bom": $bom, "schema": .}' sbom/fds.cdx.json

# Verify component integrity (SHA-256)
jq '.metadata.component.components[] | select(.hashes) | {name, hashes}' sbom/fds.cdx.json
```

**Update cycle:** Automatic via GitHub Actions on Cargo.toml/lock changes. Manual regeneration: `cargo cyclonedx --format json --target all --spec-version 1.5`
