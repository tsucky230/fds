# Package Manager Integration Guide

## 📦 Phase 3: Distribution via Package Managers

This document provides step-by-step instructions for registering fds with Windows Package Manager (winget) and Homebrew.

---

## 🪟 Windows Package Manager (winget)

### Prerequisites
- GitHub account
- Microsoft/winget-pkgs repository access

### Step 1: Prepare the Manifests

After the GitHub Release is created, copy the previous version's manifests and update the version, URLs and SHA-256 values (from `fds-X.Y.Z-checksums.sha256`, uppercase).

**Files** (multi-file format, schema 1.12.0): `packaging/winget/<version>/`
- `QraftNodeKyoto.fds.yaml` (`ManifestType: version`)
- `QraftNodeKyoto.fds.locale.en-US.yaml` (`ManifestType: defaultLocale`)
- `QraftNodeKyoto.fds.installer.yaml` (`ManifestType: installer`, `InstallerType: portable`, `Commands: [fds]`)

See [packaging/winget/0.8.0/](packaging/winget/0.8.0/) for the current set.

Validate and test locally (enabling local manifests needs an administrator shell; disable it again afterwards):
```powershell
winget validate --manifest packaging\winget\0.8.0
winget settings --enable LocalManifestFiles      # admin
winget install --manifest packaging\winget\0.8.0
winget uninstall QraftNodeKyoto.fds
winget settings --disable LocalManifestFiles     # admin
```

### Step 2: Fork & PR to microsoft/winget-pkgs

1. Fork https://github.com/microsoft/winget-pkgs
2. On a new branch, add the three files under `manifests/q/QraftNodeKyoto/fds/<version>/`
3. Open a PR titled `New package: QraftNodeKyoto.fds version 0.8.0` (later versions: `Update: QraftNodeKyoto.fds to X.Y.Z`) and fill in the PR template checklist
4. Sign the Microsoft CLA when the bot asks (first PR only)

### Step 3: Wait for Approval

- Microsoft validates the manifest
- Automated tests run (format, URL validity, etc.)
- A moderator reviews and merges

### Step 4: Users Can Install

After merge:
```bash
winget install QraftNodeKyoto.fds
```

---

## � Homebrew (macOS / Linux)

FDS is distributed through the tap [tsucky230/homebrew-tap](https://github.com/tsucky230/homebrew-tap):

```bash
brew install tsucky230/tap/fds
```

`homebrew/core` is not an option yet: a self-submission needs at least 90 forks, 90 watchers or 225 stars, and a repository younger than 30 days is normally not eligible ([Package Acceptance Policy](https://docs.brew.sh/Package-Acceptance-Policy)).

### The formula

`Formula/fds.rb` in the tap builds the tagged source tarball with Rust:

```ruby
class Fds < Formula
  desc "Lightweight, fast, keyboard-first TUI file manager"
  homepage "https://tsucky230.github.io/fds/"
  url "https://github.com/tsucky230/fds/archive/refs/tags/v0.8.0.tar.gz"
  sha256 "8113e091345140a772e3eb3e4a622b536b7bb7fd6c7f9e402df6c40ec8d63d56"
  license "GPL-3.0-only"
  head "https://github.com/tsucky230/fds.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "fds #{version}", shell_output("#{bin}/fds --version")
  end
end
```

The tap's CI (`.github/workflows/tests.yml`) runs `brew audit --strict --online`, `brew install --build-from-source`, `brew test` and `brew uninstall` on `macos-latest` and `ubuntu-latest`.

### Updating for a new release

After the GitHub Release (and its tag) exists:

1. Compute the SHA-256 of `https://github.com/tsucky230/fds/archive/refs/tags/vX.Y.Z.tar.gz` (lowercase)
2. In the tap, change `url` and `sha256` in `Formula/fds.rb` on a new branch
3. Open a PR and wait for the tap's CI to pass on macOS and Linux
4. Merge; users get it with `brew update && brew upgrade fds`

### Moving to homebrew-core later

Once the notability thresholds are met, the same formula can be proposed to [Homebrew/homebrew-core](https://github.com/Homebrew/homebrew-core) following the [Formula Cookbook](https://docs.brew.sh/Formula-Cookbook). Users would then run `brew install fds`.

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
- [x] **Homebrew**: Tap `tsucky230/homebrew-tap` created with `Formula/fds.rb`

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

### Current (v0.8.0)
- Homebrew: tap only (`brew install tsucky230/tap/fds`), built from source; not in homebrew-core
- winget: submitted ([microsoft/winget-pkgs#468247](https://github.com/microsoft/winget-pkgs/pull/468247)), waiting for review
- Not on crates.io (the name `fds` belongs to an unrelated project); use `cargo install --git https://github.com/tsucky230/fds --locked` or the GitHub Release binaries

### Future (v1.0.0+)
- 🟢 Homebrew bottles or homebrew-core formula
- 🟢 Arch Linux AUR
- 🟢 Ubuntu/Debian PPA
- 🟢 Snap store
- 🟢 Code signing for Windows binaries

---

## 💬 Questions?

- winget questions: https://github.com/microsoft/winget-pkgs/discussions
- Homebrew questions: https://discourse.brew.sh/
- fds issues: https://github.com/tsucky230/fds/issues
