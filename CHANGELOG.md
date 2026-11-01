# Changelog

English | [日本語](CHANGELOG_jp.md)

The main changes of this project.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.8.0] - 2026-11-01

This release introduces automated release workflows with multi-platform binary support and version validation.

### Added
- Automated release workflow (release.yml) with support for 5 platforms: Linux x86_64, macOS Intel/Apple Silicon, Windows x86_64/ARM64
- Pre-release validation workflow (pre-release-check.yml) ensuring version consistency across Cargo.toml, CHANGELOG files, and SBOM
- Local validation scripts (bash and PowerShell) for pre-push version verification
- Automatic GitHub Release creation with binary artifacts and SHA256 checksums
- SBOM (CycloneDX) inclusion in release artifacts for supply chain transparency
- Comprehensive release planning documentation and package manager integration guides

### Changed
- Enhanced CI and SBOM workflows with detailed inline documentation and comments
- Improved version management with automated checks before release

## [0.7.0] - 2026-10-31

This release adds the ability to type and execute commands within FDS.

### Added
- `!` lets you type a command and run it in the current folder. Pipes and wildcards are supported, and Ctrl+C stops only the command. After it finishes, press Enter to return to the list and reload. Use `↑` / `↓` to go through the commands of this session (R-412)
- `x` opens the box filled in for the marks (` {files}`), an executable file (`./build.sh `) or the cursor entry (` {file}`). Names are quoted, so one with spaces or symbols still arrives as one argument (R-412)
- A line consisting only of `cd <path>` changes FDS's location (R-412)

- FDS is licensed under GPL-3.0-only: `LICENSE`, and a notice on the Help screen and in `--help`
- `sbom/fds.cdx.json`: the dependencies as a CycloneDX SBOM, kept up to date by GitHub Actions

### Changed
- The title band shows `© 2026 Qraft Node Kyoto` (R-200-12)
- `README.md` and `CHANGELOG.md` are now in English; the Japanese versions are `README_jp.md` and `CHANGELOG_jp.md`

### Fixed
- Pressing Ctrl+C while `o` or the external editor is running terminated FDS as well (R-412-7)

## [0.6.0] - 2026-10-31

### Added
- View, create, and extract ZIP / tar / tar.gz / tgz archives without external tools. Use `p` to create and `u` to extract
- Display a top banner with name, version, `©Qraft Node Kyoto`, and the current time (R-200-12). The banner is omitted when the height is less than 15
- Pressing Tab on a directory splits the right preview vertically, moving the cursor to the upper list and showing the contents of the cursor entry below (R-200-13). In this mode you can only navigate and inspect; file operations are not allowed. The list starts with `[..]`; pressing Enter there or Tab / Esc returns to the left side. Pressing Enter on a directory moves the left list as well
- When q / F10 / Esc is pressed in the list, a confirmation prompt appears at the bottom instead of exiting immediately. Press y or Enter to exit, n or Esc to cancel (R-200-14)

- `o` opens the entry with the OS default application. Executable files require confirmation with `(y/N)`, and are run only if y is pressed (R-110-3)
- `[open.by_extension]` allows configuring the application opened with Enter per file extension. Setting `wait = false` launches without waiting (R-110-4)

### Changed
- Changed binary preview from hex dump to a single line showing size. Press Enter to open the traditional hex Viewer
- Directory display changed from `[DIR] name` to `[name]/`, freeing list width. Rendered in bold cyan, distinguishing them from a file named `[a]` (R-200-11)
- Scrollbar changed from a white block to a thin line, and no groove is drawn
- Removed the ` fds ` label from the top border (moved to the banner)

## [0.5.1] - 2026-10-25

This release adds version reporting and command-line help.

### Added
- `fds --version` (`-V`) shows the version, and `fds --help` (`-h`) displays usage, key list, and config file location (R-111-2). Anything after `--` is treated as a path, not an option
- Show the version in the help screen (F1) header (R-111-3)

## [0.5.0] - 2026-10-24

This release shows a preview to the right of the list. The cursor entry remains a single point; it is not a dual-pane view.

### Added
- preview: On terminals at least 80 columns wide, a preview of the cursor entry is shown to the right of the list (R-200-9). Text and Markdown show the first 64 KiB, binaries show the leading bytes as hex, images display format and dimensions, and folders list their contents (up to 1000 items). No external Packs are used, and FIFO or device files are not opened. The preview is enabled by default, toggled with `P`, and can be disabled with `[ui] preview = false`. The cursor entry remains a single point; this is not a dual-pane view
- preview images: PNG / JPEG are rendered directly in the right pane (R-200-10). Files ≤ 16 MiB are read and scaled in a background thread (long side 512 px). Rendering follows `[viewer] image_protocol`, and can be switched with `p` even in the list. Setting `[ui] preview_images = false` reverts to showing only format and size
- vi-style keys in Viewer: With `[viewer] vi_keys = true`, h / l scroll horizontally (R-500-6). Backspace, q, Esc, F3, and v still close the viewer

### Changed
- The information panel spans the full width below the list and preview
- In Viewer, h / Backspace also return to the list (same direction as the list's "up one level". l opens, h returns)
- When the right pane is not visible (width < 80, or help or Viewer is displayed), the preview is not loaded

## [0.4.0] - 2026-10-20

Phase 4 (Supporter Infrastructure).

### Added
- Supporter license: The file `supporter.license` (`fds-license-v1`) in the config directory is verified offline with an Ed25519 signature using a public key embedded in the binary. No network access is required. The Core functions normally even without a license or with a corrupted one. The production public key is not yet included
- Pack: External commands defined in `<config directory>/packs/<folder>/pack.toml` add per-extension display. The command is invoked without a shell, with `{path}` replaced by the file's absolute path. Output is limited to 8 MiB; it can be aborted by timeout (default 10 s, `[providers] timeout_secs`) or q / Esc while reading. Tab switches to the built-in Viewer
- Official Packs (`official = true`) are loaded only when both a signature (`pack.toml.sig`) and a license are present. Community Packs load without a license. They can be disabled with `[packs] disabled`
- The help screen (F1) includes a Supporter section below, showing license status and each Pack's status and why it was not loaded

### Changed
- If the best-matching Viewer cannot open a file and the next Viewer is used (e.g., Pack command failure, timeout, or opening a corrupted image as hex), the status line shows "Cannot display with (Viewer name): (reason)"
- Messages inside the Viewer are cleared with the next key press, returning to character-set and line-number display

## [0.3.1] - 2026-10-20

Fixes pending items from Phase 3 and bugs discovered during manual testing on Windows hardware (M22–M34).

### Added
- Setting `[ops] delete = "trash"` makes F8 move items to the trash (Windows trash, macOS Finder trash, Linux freedesktop Trash). Items that cannot be trashed remain as errors and are not switched to permanent delete. An unknown value is treated as trash, with a notice at startup
- Pressing E in the Viewer cycles the character set (auto → UTF-8 → Shift_JIS → EUC-JP → ISO-2022-JP → UTF-16LE → UTF-16BE → Windows-1252 → auto). The current position is preserved. The status line shows "(manual)"
- During Copy / Move, display byte counts (e.g., `1.1 GiB / 4.0 GiB`)
- Show an information panel below the list (toggle with `i`): type, permissions (Unix shows `755 rwxr-xr-x` and owner, Windows shows R/H/S/A attributes), read-only flag, size, modification/creation dates, symlink target and broken status, folder item count. When items are marked, also show count and total size
- Assign ← to "go to parent directory" in both the list and destination selection (same as h / Backspace). In Viewer, ← continues to scroll horizontally as before

### Changed
- The bottom key guide is shown only in the list and destination selection. It is hidden on screens where list keys are inactive, such as confirmations, conflicts, operations, or pop-ups
- While a drive/location pop-up is open, keep the count of items to Copy / Move displayed in the status line
- In the key guide for narrow screens, move "q quit" to the front (it was truncated at widths 40–50)
- Remove the "(Phase 2)" and "(Phase 3)" labels from the help key list
- Change the delete confirmation to "Permanently delete n items?" (when trash is configured, it reads "Move n items to trash?")

### Fixed
- The key guide for destination selection incorrectly said "←/h: up", but ← did not work
- In the information panel, a long link target caused the "broken link" label to be truncated at line end
- When Copy / Move is aborted or fails, empty folders created by the operation remained at the destination (folders with contents or original folders are retained)

## [0.3.0] - 2026-09-24

Release of Phase 3 (File Operations).

### Added
- F5 / c → Copy, F6 / m → Move, F8 / d → Delete (with confirmation), F2 / r → Rename, F7 / n → New folder
- `/` initiates search (moves to matching entries as you type; Esc returns to the original position)
- F4 / e opens the external editor. Uses `[editor] command` and per-extension settings; if none, falls back to VISUAL → EDITOR → OS default
- Copy / Move enters destination selection. The top banner shows "what / which operation", the destination path, and the relationship to the source (same / above / inside / different drive). Press Enter on the "Copy / Move here" line at the top of the list to execute. The bottom key guide changes according to the cursor position
- L / F9 pops up a list of drives, home, and mount points (a way to switch drives on Windows)
- Copy writes to a temporary name (`.<name>.fds-partial.<pid>-<job>`) before publishing, ensuring no partial files remain on abort or failure. Works on FAT32 / exFAT
- Move performs a rename on the same filesystem; on different filesystems it copies and deletes the source only after the copy completes
- When a name conflict occurs, you can choose Overwrite, Skip, Rename (`name (2).ext`), or Abort. "Apply to all remaining" is also available
- Progress is shown during execution; Esc aborts. Failures appear in the error list
- Rename, New folder, and Search are displayed in framed input boxes with an inverted cursor
- The entire screen is surrounded by a box frame. It can be disabled with `[ui] frame = false`. No frame is shown when width < 42 or height < 12
- When the file list is long, a scrollbar appears on the right edge
- Marking items causes available actions (e.g., F5 Copy) to be shown in the status line

### Changed
- Keys like F2 / F7, previously unassigned through Phase 2, are now bound to actions
- Copy / Move into the same directory is rejected before starting. Symlinks are duplicated as links, and Delete removes only the link. Moving into the same folder does nothing

## [0.2.0] - 2026-09-23

Release of Phase 2 (Universal Viewer).

### Added
- F3 / v (or Enter on a regular file) opens a full-screen Viewer for text, Markdown, hex, and images (PNG / JPEG)
- Automatic detection of text character set (UTF-8 / UTF-16 / Shift_JIS / EUC-JP / ISO-2022-JP). The detected set is shown in the status line
- Tab switches the same file to a different Viewer, w toggles line wrapping, ← / → scroll horizontally
- Text larger than 8 MiB shows only the first 8 MiB. Hex reads only the displayed range, even for multi-GB files
- Image rendering can be set via `[viewer] image_protocol` (auto / kitty / sixel / iterm2 / halfblocks / none)
- Press p while displaying an image to cycle the rendering mode (half-block / Sixel / Kitty / iTerm2)
- The rendering mode is auto-selected from the environment (Windows Terminal uses Sixel, Kitty / Ghostty use Kitty, iTerm2 / WezTerm use iTerm2, tmux and others use half-block)

### Changed
- Ctrl+C exits FDS from any screen. q / F10 / Esc close the Viewer

## [0.1.0] - 2026-09-23

Initial release of Phase 1 (Core Filer).

### Added
- Single-pane directory list (`[..]` → directories → files) in case-insensitive natural order
- Cursor movement (↑↓ / j k, PageUp/PageDown / Ctrl+B Ctrl+F, Home/End / g G) and scrolling
- Directory navigation (Enter / l, Backspace / h). Returning to the parent restores the cursor to the previously visited directory
- Mark (Space). Moving into a directory clears marks; they persist across reloads
- Toggle hidden files (`.`), reload (Ctrl+R), key list (F1 / ?)
- `fds [path]` starts FDS in the specified directory
- Responsive layout based on screen size (shows a warning below 40×10, and restores when resized)
- Accurate width calculation and truncation for filenames containing Japanese, emojis, or combining characters; control characters are sanitized
- Display the target type of symlinks (directory / file / broken link)
- Multilingual support (built-in English and Japanese; switch via `[ui] language` or OS locale; additional or overriding languages via language files)
- Restores terminal state on normal exit, error, panic, SIGINT/SIGTERM/SIGHUP, SSH disconnect, or Ctrl+Z (Unix)
- CI for Windows, macOS, and Linux

[0.7.0]: https://github.com/tsucky230/fds/releases/tag/v0.7.0
[0.6.0]: https://github.com/tsucky230/fds/releases/tag/v0.6.0
[0.5.1]: https://github.com/tsucky230/fds/releases/tag/v0.5.1
[0.5.0]: https://github.com/tsucky230/fds/releases/tag/v0.5.0
[0.4.0]: https://github.com/tsucky230/fds/releases/tag/v0.4.0
[0.3.1]: https://github.com/tsucky230/fds/releases/tag/v0.3.1
[0.3.0]: https://github.com/tsucky230/fds/releases/tag/v0.3.0
[0.2.0]: https://github.com/tsucky230/fds/releases/tag/v0.2.0
[0.1.0]: https://github.com/tsucky230/fds/releases/tag/v0.1.0
