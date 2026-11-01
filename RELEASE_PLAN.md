# fds Release & Version Management Plan

**Status**: 📋 Planning Phase  
**Last Updated**: 2026-11-01  
**Next Version**: 0.8.0 (proposed)

---

## 📌 Current State

### Version Inventory
| File | Current | Status |
|------|---------|--------|
| `Cargo.toml` | 0.7.0 | ✅ Single source of truth |
| `sbom/fds.cdx.json` | 0.7.0 | ✅ Auto-synced via CI |
| `CHANGELOG.md` | Latest entry for v0.7.0 | ✅ Maintained |
| `CHANGELOG_jp.md` | Latest entry for v0.7.0 | ✅ Maintained |
| GitHub Releases | Not published | ❌ **Missing** |
| Git Tags | Not created | ❌ **Missing** |
| Package Managers | Not registered | ❌ **Missing** |

### Problems to Solve
1. **No version check before push** — Risk of version mismatch
2. **No automated release** — Manual process prone to errors
3. **No binary distribution** — Only source code available
4. **No package manager support** — Can't install via `winget`, `homebrew`, `cargo`
5. **No checksums** — No integrity verification for downloads

---

## 🎯 Phase 1: Version Validation (Immediate)

### 1.1 Pre-Push Version Check Hook

**Goal**: Ensure version consistency before any push  
**Trigger**: `git push` (via pre-push hook or CI)

**Checklist to Validate**:
```
□ Cargo.toml version
□ CHANGELOG.md has entry for current version
□ CHANGELOG_jp.md has entry for current version
□ sbom/fds.cdx.json version (if regenerated)
□ Version follows Semantic Versioning (X.Y.Z)
```

**Implementation Option A: Local Git Hook** (Developer level)
```bash
#!/bin/bash
# .git/hooks/pre-push
VERSION=$(grep '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')
if ! grep -q "## \[$VERSION\]" CHANGELOG.md; then
  echo "❌ Version $VERSION not found in CHANGELOG.md"
  exit 1
fi
echo "✓ Version validation passed: $VERSION"
```

**Implementation Option B: CI Validation** (Automatic)
```yaml
# .github/workflows/pre-release-check.yml
on:
  push:
    branches: [main]

jobs:
  validate-version:
    runs-on: ubuntu-latest
    steps:
      - name: Extract Cargo.toml version
        run: echo "version=$VERSION" >> $GITHUB_OUTPUT
      - name: Validate CHANGELOG entries
        run: |
          grep -q "## \[$VERSION\]" CHANGELOG.md || exit 1
          grep -q "## \[$VERSION\]" CHANGELOG_jp.md || exit 1
          echo "✓ Version $VERSION found in both CHANGELOG files"
      - name: Validate SBOM version
        run: |
          SBOM_VERSION=$(jq -r '.metadata.component.version' sbom/fds.cdx.json)
          [ "$SBOM_VERSION" = "$VERSION" ] || exit 1
          echo "✓ SBOM version matches: $SBOM_VERSION"
```

---

## 🚀 Phase 2: Automated Release (Sprint 1)

### 2.1 Release Workflow

**Trigger**: Tag creation (`git tag v0.8.0` → `git push --tags`)

**Release Process** (`release.yml`):
```
Tag pushed (e.g., v0.8.0)
        ↓
[Step 1] Extract version & validate changelog
        ↓
[Step 2] Build binaries for Windows / macOS / Linux
        ↓
[Step 3] Generate checksums (SHA256)
        ↓
[Step 4] Create GitHub Release with:
         - Release notes from CHANGELOG
         - Binary attachments
         - Checksum file
        ↓
[Step 5] Publish to crates.io (Rust)
        ↓
[Step 6] Trigger winget / homebrew update
```

### 2.2 Binary Build Matrix

```yaml
strategy:
  matrix:
    include:
      # Linux x86_64
      - os: ubuntu-latest
        target: x86_64-unknown-linux-gnu
        artifact_name: fds
      
      # macOS (Intel)
      - os: macos-latest
        target: x86_64-apple-darwin
        artifact_name: fds
      
      # macOS (Apple Silicon)
      - os: macos-latest
        target: aarch64-apple-darwin
        artifact_name: fds
      
      # Windows x86_64
      - os: windows-latest
        target: x86_64-pc-windows-msvc
        artifact_name: fds.exe
      
      # Windows ARM64 (Surface devices)
      - os: windows-latest
        target: aarch64-pc-windows-msvc
        artifact_name: fds.exe
```

### 2.3 Checksum & Integrity

**SHA256 Checksums** (distributed with release):
```
fds-0.8.0-x86_64-unknown-linux-gnu: abc123...
fds-0.8.0-x86_64-apple-darwin:      def456...
fds-0.8.0-aarch64-apple-darwin:     ghi789...
fds-0.8.0-x86_64-pc-windows-msvc:   jkl012...
fds-0.8.0-aarch64-pc-windows-msvc:  mno345...
sbom-0.8.0.json:                    pqr678...
```

**Usage**:
```bash
# Verify after download
sha256sum -c fds-0.8.0-checksums.sha256
# Output: fds-0.8.0-x86_64-unknown-linux-gnu: OK
```

---

## 📦 Phase 3: Package Manager Integration (Sprint 2)

### 3.1 Rust Crates Registry (`cargo install fds`)

**Status**: 🟡 Partial (crate exists but not published)

**Steps**:
1. Create `Cargo.lock` in repository (already done ✓)
2. Ensure `license`, `repository`, `homepage` in Cargo.toml (done ✓)
3. Run: `cargo publish --allow-dirty` (from CI after GitHub Release created)

**User will run**:
```bash
cargo install fds --locked
```

### 3.2 Windows Package Manager (`winget`)

**Status**: ❌ Not registered

**Registration Process**:
1. Fork `microsoft/winget-pkgs` repository
2. Create manifest file: `manifests/q/QraftNodeKyoto/fds/0.8.0.yaml`
3. Example manifest:
```yaml
PackageIdentifier: QraftNodeKyoto.fds
PackageVersion: 0.8.0
PackageName: fds
Publisher: Qraft Node Kyoto
PackageUrl: https://github.com/tsucky230/fds
License: GPL-3.0-only
LicenseUrl: https://github.com/tsucky230/fds/blob/main/LICENSE
Homepage: https://tsucky230.github.io/fds/
Installers:
  - Architecture: x64
    InstallerUrl: https://github.com/tsucky230/fds/releases/download/v0.8.0/fds-0.8.0-x86_64-pc-windows-msvc.exe
    InstallerSha256: abc123...
    InstallerType: portable
  - Architecture: arm64
    InstallerUrl: https://github.com/tsucky230/fds/releases/download/v0.8.0/fds-0.8.0-aarch64-pc-windows-msvc.exe
    InstallerSha256: def456...
    InstallerType: portable
```

**User will run**:
```bash
winget install QraftNodeKyoto.fds
```

### 3.3 macOS Homebrew

**Status**: ❌ Not registered

**Registration Process**:
1. Create Homebrew formula: `fds.rb`
2. Submit to `homebrew/core` (or `homebrew/tap`)
3. Formula template:
```ruby
class Fds < Formula
  desc "Keyboard-First TUI File Manager"
  homepage "https://tsucky230.github.io/fds/"
  url "https://github.com/tsucky230/fds/archive/refs/tags/v0.8.0.tar.gz"
  sha256 "abc123..."
  license "GPL-3.0-only"

  depends_on "rust" => :build

  def install
    system "cargo", "install", "--locked", "--root", prefix, "--path", "."
  end
end
```

**User will run**:
```bash
brew install fds
```

### 3.4 Linux Distributions

**Status**: ❌ Not registered

**Potential Targets**:
- **Arch Linux** (AUR - Arch User Repository)
- **Ubuntu/Debian** (PPA or official repos)
- **Fedora** (Copr)

Each has different requirements; can be added incrementally.

---

## 🔄 Phase 4: Version Update Workflow (Ongoing)

### 4.1 How to Bump Version

**Semver Rules**:
- **MAJOR** (X.0.0): Breaking changes
- **MINOR** (0.Y.0): New features (backward compatible)
- **PATCH** (0.0.Z): Bug fixes

**Process** (before `git push origin main`):

1. **Update Cargo.toml**:
```toml
[package]
version = "0.8.0"  # Changed from 0.7.0
```

2. **Update CHANGELOG.md**:
```markdown
## [0.8.0] - 2026-11-15

### Added
- Feature 1
- Feature 2

### Fixed
- Bug 1

### Changed
- Breaking change if any
```

3. **Update CHANGELOG_jp.md** (Japanese):
```markdown
## [0.8.0] - 2026-11-15

### 追加
- 機能1
- 機能2

### 修正
- バグ1

### 変更
- 互換性を破る変更（ある場合）
```

4. **Commit**:
```bash
git add Cargo.toml CHANGELOG.md CHANGELOG_jp.md
git commit -m "Bump version to 0.8.0"
git push origin main
```

5. **CI runs version validation** ✓ (Phase 1)

6. **After tests pass, create release tag**:
```bash
git tag -a v0.8.0 -m "Release version 0.8.0"
git push origin v0.8.0
```

7. **CI runs release workflow** ✓ (Phase 2)
   - Builds binaries
   - Creates GitHub Release
   - Publishes to crates.io

8. **Manual (or automated) package manager updates**:
   - winget manifest (can be automated)
   - homebrew formula (community-driven or manual)

---

## 📋 Implementation Timeline

| Phase | Feature | Effort | Timeline | Owner |
|-------|---------|--------|----------|-------|
| **1** | Version Check Hook | Small | Week 1 | CI/CD |
| **1** | Pre-Release CI Validation | Small | Week 1 | CI/CD |
| **2** | Build Binaries (release.yml) | Medium | Week 2-3 | CI/CD |
| **2** | GitHub Release Creation | Small | Week 2-3 | CI/CD |
| **2** | Checksum Generation | Small | Week 2-3 | CI/CD |
| **2** | crates.io Publishing | Small | Week 2-3 | CI/CD |
| **3** | winget Manifest & Registration | Medium | Week 3-4 | Manual |
| **3** | Homebrew Formula & Registration | Medium | Week 3-4 | Manual |
| **3** | AUR/Linux Distro Registration | Medium | Sprint 2+ | Manual |

---

## 🛡️ Security Considerations

1. **Checksum Verification**: All downloads should verify SHA256
2. **Code Signing** (Optional): Sign binaries with certificate
3. **SBOM in Release**: Include `sbom-0.8.0.json` for supply chain transparency
4. **Vulnerability Scan**: Run `grype` before release
5. **GitHub Release Artifacts**: Ensure all checksums are in release notes

---

## ✅ Acceptance Criteria

**Phase 1 Complete** when:
- [ ] Pre-push version check passes
- [ ] CI validates version consistency

**Phase 2 Complete** when:
- [ ] Binaries build for 5 targets
- [ ] GitHub Release created with all artifacts
- [ ] Checksums generated and verified
- [ ] crates.io publishes automatically

**Phase 3 Complete** when:
- [ ] `winget install QraftNodeKyoto.fds` works
- [ ] `brew install fds` works
- [ ] `cargo install fds --locked` works

---

## 📚 References

- [Semantic Versioning](https://semver.org/)
- [Keep a Changelog](https://keepachangelog.com/)
- [Rust crates.io Publishing](https://doc.rust-lang.org/cargo/publishing/)
- [Windows Package Manager](https://learn.microsoft.com/en-us/windows/package-manager/)
- [Homebrew Formula Development](https://docs.brew.sh/Formula-Cookbook)

---

## 🤔 Questions for Review

1. Should we automate winget registration, or keep it manual?
2. Should we sign binaries with a certificate?
3. Should we create a `choco` (Chocolatey) package for Windows?
4. Should we support other architectures (riscv64, s390x)?
5. Should we maintain binary archives (e.g., `.tar.gz`, `.zip`) separate from installers?
