# Package Manager Integration Guide

## 📦 Phase 3: Distribution via Package Managers

This document provides step-by-step instructions for registering fds with Windows Package Manager (winget) and Homebrew.

---

## 🪟 Windows Package Manager (winget)

### Prerequisites
- GitHub account
- Microsoft/winget-pkgs repository access

### Step 1: Create Manifest File

After GitHub Release is created, prepare the winget manifest:

**File**: `winget/manifests/q/QraftNodeKyoto/fds/0.8.0.yaml`

```yaml
PackageIdentifier: QraftNodeKyoto.fds
PackageVersion: 0.8.0
PackageName: fds
Publisher: Qraft Node Kyoto
PublisherUrl: https://github.com/tsucky230
PublisherSupportUrl: https://github.com/tsucky230/fds/issues
Author: Qraft Node Kyoto
License: GPL-3.0-only
LicenseUrl: https://github.com/tsucky230/fds/blob/main/LICENSE
Homepage: https://tsucky230.github.io/fds/
Description: "Keyboard-First TUI File Manager - Lightweight terminal file browser with preview capabilities"
Tags:
  - file-manager
  - tui
  - keyboard-driven
  - terminal
  - cli

ReleaseNotes: |
  [Release notes from GitHub Release]

ReleaseNotesUrl: https://github.com/tsucky230/fds/releases/tag/v0.8.0

Installers:
  - Architecture: x64
    InstallerType: portable
    InstallerUrl: https://github.com/tsucky230/fds/releases/download/v0.8.0/fds-0.8.0-x86_64-pc-windows-msvc.exe
    InstallerSha256: abc123def456...  # From release checksums
    InstallerLocale: en-US
    
  - Architecture: arm64
    InstallerType: portable
    InstallerUrl: https://github.com/tsucky230/fds/releases/download/v0.8.0/fds-0.8.0-aarch64-pc-windows-msvc.exe
    InstallerSha256: ghi789jkl012...  # From release checksums

ManifestType: singleton
ManifestVersion: 1.6.0
```

### Step 2: Fork & PR to microsoft/winget-pkgs

1. Fork: https://github.com/microsoft/winget-pkgs
2. Clone your fork:
   ```bash
   git clone https://github.com/YOUR_USERNAME/winget-pkgs.git
   cd winget-pkgs
   ```
3. Create branch:
   ```bash
   git checkout -b fds-0.8.0
   ```
4. Add manifest:
   ```bash
   mkdir -p manifests/q/QraftNodeKyoto/fds/0.8.0
   # Place your YAML file in above directory
   ```
5. Validate locally:
   ```bash
   # Install Windows Package Manager CLI
   winget source reset --force
   ```
6. Commit & Push:
   ```bash
   git add manifests/q/QraftNodeKyoto/fds/0.8.0/QraftNodeKyoto.fds.yaml
   git commit -m "Add fds v0.8.0"
   git push origin fds-0.8.0
   ```
7. Create PR on https://github.com/microsoft/winget-pkgs/pulls

### Step 3: Wait for Approval

- Microsoft validates the manifest
- Automated tests run (format, URL validity, etc.)
- Merge within 1-2 weeks typically

### Step 4: Users Can Install

After merge:
```bash
winget install QraftNodeKyoto.fds
```

---

## 🍎 Homebrew (macOS)

### Prerequisites
- Homebrew formula knowledge
- GitHub account
- macOS development environment

### Step 1: Create Homebrew Formula

**File**: `homebrew-fds/Formula/fds.rb`

```ruby
class Fds < Formula
  desc "Keyboard-First TUI File Manager"
  homepage "https://tsucky230.github.io/fds/"
  url "https://github.com/tsucky230/fds/archive/refs/tags/v0.8.0.tar.gz"
  sha256 "abc123def456..."  # Get from release assets or:
                             # curl -sL URL | sha256sum
  license "GPL-3.0-only"
  
  depends_on "rust" => :build
  
  def install
    system "cargo", "install", "--locked", "--root", prefix, "--path", "."
  end
  
  test do
    assert_match "fds", shell_output("#{bin}/fds --version")
  end
end
```

### Step 2: Calculate SHA256

```bash
# Get the tar.gz SHA256
curl -sL "https://github.com/tsucky230/fds/archive/refs/tags/v0.8.0.tar.gz" | sha256sum
# Output: abc123def456...
```

### Step 3: Test Formula Locally

```bash
# Option A: Using homebrew-core (official)
brew tap homebrew/core
brew install --HEAD fds

# Option B: Create your own tap
# Create repository: tsucky230/homebrew-fds
# Add formula to: Formula/fds.rb
# Test with:
brew tap tsucky230/fds
brew install fds --HEAD
```

### Step 4: Submit to Homebrew Core (Optional)

1. Fork: https://github.com/Homebrew/homebrew-core
2. Follow: https://docs.brew.sh/Formula-Cookbook
3. Add formula to `Formula/fds.rb`
4. Test:
   ```bash
   brew install --verbose --build-from-source Formula/fds.rb
   ```
5. PR to https://github.com/Homebrew/homebrew-core

### Step 5: Users Can Install

After approval:
```bash
brew install fds
# or with custom tap:
brew tap tsucky230/fds
brew install fds
```

---

## 🐧 Linux Package Managers (Optional)

### Arch Linux (AUR)

**Steps**:
1. Create `PKGBUILD` file
2. Register on https://aur.archlinux.org/
3. Users: `yay -S fds` or `paru -S fds`

Example `PKGBUILD`:
```bash
pkgname=fds
pkgver=0.8.0
pkgrel=1
pkgdesc="Keyboard-First TUI File Manager"
arch=('x86_64')
url="https://github.com/tsucky230/fds"
license=('GPL3')
depends=('gcc-libs')
makedepends=('rust')
source=("$pkgname-$pkgver.tar.gz::https://github.com/tsucky230/fds/archive/v$pkgver.tar.gz")
sha256sums=('abc123...')

build() {
  cd "$pkgname-$pkgver"
  cargo build --release --locked
}

package() {
  cd "$pkgname-$pkgver"
  install -Dm755 target/release/fds "$pkgdir/usr/bin/fds"
  install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
```

### Ubuntu/Debian (PPA)

**Steps**:
1. Setup Launchpad account
2. Create PPA: https://launchpad.net/~YOUR_USERNAME/+create-new-ppa
3. Build and upload `.deb` packages

(More complex; recommend after v1.0.0 release)

---

## 📋 Checklist for Release v0.8.0

### Before Creating Tag
- [ ] Version bumped in `Cargo.toml`
- [ ] CHANGELOG.md updated
- [ ] CHANGELOG_jp.md updated
- [ ] All tests pass: `cargo test --all`
- [ ] Code formatted: `cargo fmt`
- [ ] No clippy warnings: `cargo clippy -- -D warnings`

### After GitHub Release Created
- [ ] Download release assets and verify checksums
- [ ] Test binary on Windows / macOS / Linux
- [ ] Check release notes on GitHub

### Week 1-2: Package Manager Registration
- [ ] **winget**: Create PR to microsoft/winget-pkgs
- [ ] **Homebrew**: Create tap (tsucky230/homebrew-fds) or submit to homebrew-core

### Week 2-3: Announce
- [ ] Post release on GitHub Discussions
- [ ] Share on Twitter / Qiita / dev.to
- [ ] Update README with installation methods

### Week 3+: Monitor
- [ ] Monitor GitHub Issues for bug reports
- [ ] Update winget/Homebrew manifests for patch releases
- [ ] Prepare for v0.9.0 planning

---

## 🔗 Useful References

- [Windows Package Manager](https://learn.microsoft.com/en-us/windows/package-manager/winget/)
- [Homebrew Formula Cookbook](https://docs.brew.sh/Formula-Cookbook)
- [Arch Linux PKGBUILD](https://wiki.archlinux.org/title/PKGBUILD)
- [Keep a Changelog](https://keepachangelog.com/)
- [Semantic Versioning](https://semver.org/)

---

## ⚠️ Known Limitations

### Current (v0.7.0)
- No Homebrew formula yet (manual build required)
- No winget registration
- Only available via `cargo install --git https://github.com/tsucky230/fds --locked` or manual download (the crates.io name `fds` belongs to an unrelated project)

### Planned (v0.8.0+)
- ✅ Automated release workflow
- ✅ Binary distribution via GitHub Releases
- 🟡 winget registration (manual, week 1-2)
- 🟡 Homebrew tap (manual, week 1-2)

### Future (v1.0.0+)
- 🟢 Official Homebrew formula
- 🟢 Arch Linux AUR
- 🟢 Ubuntu/Debian PPA
- 🟢 Snap store
- 🟢 Code signing for Windows binaries

---

## 💬 Questions?

- winget questions: https://github.com/microsoft/winget-pkgs/discussions
- Homebrew questions: https://discourse.brew.sh/
- fds issues: https://github.com/tsucky230/fds/issues
