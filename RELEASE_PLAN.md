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
VERSION=$(awk -F'"' '/^\[/{p=($0=="[package]")} p && /^version *=/{print $2; exit}' Cargo.toml)
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
        id: version
        run: |
          VERSION=$(awk -F'"' '/^\[/{p=($0=="[package]")} p && /^version *=/{print $2; exit}' Cargo.toml)
          echo "version=$VERSION" >> $GITHUB_OUTPUT
      - name: Validate CHANGELOG entries
        run: |
          grep -q "## \[$VERSION\]" CHANGELOG.md || exit 1
          grep -q "## \[$VERSION\]" CHANGELOG_jp.md || exit 1
          echo "✓ Version $VERSION found in both CHANGELOG files"
      - name: Validate SBOM version (warning only; sbom.yml commits the regenerated SBOM after a bump)
        run: |
          SBOM_VERSION=$(jq -r '.metadata.component.version' sbom/fds.cdx.json)
          [ "$SBOM_VERSION" = "$VERSION" ] || echo "::warning::SBOM is $SBOM_VERSION"
```

Only the `[package]` section is read: `[[package.metadata.cyclonedx.components]]` also has `version =` lines.

---

## 🚀 Phase 2: Automated Release (Sprint 1)

### 2.1 Release Workflow

**Trigger**: Tag creation (`git tag v0.8.0` → `git push origin v0.8.0`).
Running the workflow manually (`workflow_dispatch`) is a dry run: it validates, tests, builds and checksums, but creates no Release.

**Release Process** (`release.yml`):
```
Tag pushed (e.g., v0.8.0)
        ↓
[Step 1] Tag = Cargo.toml version, CHANGELOG entries, SBOM version, cargo test
        ↓
[Step 2] Build binaries for Windows / macOS / Linux
        ↓
[Step 3] Generate checksums (SHA256)
        ↓
[Step 4] Create GitHub Release with:
         - Release notes from CHANGELOG
         - Binary attachments
         - Checksum file
         - SBOM (fds-X.Y.Z-sbom.cdx.json)
        ↓
[Step 5] winget / homebrew update (manual, Phase 3)
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

**SHA256 Checksums** (`fds-0.8.0-checksums.sha256`, `sha256sum` format):
```
abc123...  fds-0.8.0-aarch64-apple-darwin
def456...  fds-0.8.0-aarch64-pc-windows-msvc.exe
ghi789...  fds-0.8.0-x86_64-apple-darwin
jkl012...  fds-0.8.0-x86_64-pc-windows-msvc.exe
mno345...  fds-0.8.0-x86_64-unknown-linux-gnu
```

**Usage**:
```bash
# Verify after download (only the files you downloaded)
sha256sum -c --ignore-missing fds-0.8.0-checksums.sha256
# Output: fds-0.8.0-x86_64-unknown-linux-gnu: OK
```

---

## 📦 Phase 3: Package Manager Integration (Sprint 2)

### 3.1 Rust / Cargo

**Status**: Not published to crates.io. The crate name `fds` is already taken there by an unrelated project, so `cargo install fds` would install someone else's tool.

**User will run**:
```bash
cargo install --git https://github.com/tsucky230/fds --tag v0.8.0 --locked
```

### 3.2 Windows Package Manager (`winget`)

**Status**: ❌ Not registered

**Registration Process**: see [PACKAGE_MANAGER_GUIDE.md](PACKAGE_MANAGER_GUIDE.md).
The manifests (multi-file, schema 1.12.0) live in [packaging/winget/0.8.0/](packaging/winget/0.8.0/) and go to
`manifests/q/QraftNodeKyoto/fds/0.8.0/` in a PR to `microsoft/winget-pkgs`.

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

4. **Commit** (include `Cargo.lock`; release builds use `--locked`):
```bash
cargo check --locked   # updates nothing if Cargo.lock is already in sync
git add Cargo.toml Cargo.lock CHANGELOG.md CHANGELOG_jp.md
git commit -m "Bump version to 0.8.0"
git push origin main
```

5. **CI runs**: CI (tests), Pre-Release Validation, and SBOM. SBOM commits the regenerated `sbom/fds.cdx.json`.

6. **Dry run** after `git pull` (picks up the SBOM commit):
   Actions → Release → Run workflow. All 5 builds and checksums must pass.

7. **Tag the commit that contains the SBOM update**:
```bash
git tag -a v0.8.0 -m "Release version 0.8.0"
git push origin v0.8.0
```

8. **CI runs release workflow** ✓ (Phase 2)
   - Builds binaries
   - Creates GitHub Release

9. **Manual (or automated) package manager updates**:
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
| **3** | winget Manifest & Registration | Medium | Week 3-4 | Manual |
| **3** | Homebrew Formula & Registration | Medium | Week 3-4 | Manual |
| **3** | AUR/Linux Distro Registration | Medium | Sprint 2+ | Manual |

---

## 🛡️ Security Considerations

1. **Checksum Verification**: All downloads should verify SHA256
2. **Code Signing** (Optional): Sign binaries with certificate
3. **SBOM in Release**: Include `fds-0.8.0-sbom.cdx.json` for supply chain transparency
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

**Phase 3 Complete** when:
- [ ] `winget install QraftNodeKyoto.fds` works
- [ ] `brew install fds` works
- [ ] `cargo install --git https://github.com/tsucky230/fds --locked` works

---

## 📚 References

- [Semantic Versioning](https://semver.org/)
- [Keep a Changelog](https://keepachangelog.com/)
- [Windows Package Manager](https://learn.microsoft.com/en-us/windows/package-manager/)
- [Homebrew Formula Development](https://docs.brew.sh/Formula-Cookbook)

---

## 🤔 Questions for Review

1. Should we automate winget registration, or keep it manual?
2. Should we sign binaries with a certificate?
3. Should we create a `choco` (Chocolatey) package for Windows?
4. Should we support other architectures (riscv64, s390x)?
5. Should we maintain binary archives (e.g., `.tar.gz`, `.zip`) separate from installers?
