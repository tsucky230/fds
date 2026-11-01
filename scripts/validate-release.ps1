# Local version validation script for fds release workflow (PowerShell)
# This script verifies all version consistency checks before pushing

$ErrorActionPreference = "Stop"

Write-Host ""
Write-Host "════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host "🔍 fds Release Validation Script" -ForegroundColor Cyan
Write-Host "════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host ""

# Step 1: Extract Cargo.toml version
Write-Host "[1/5] Extracting version from Cargo.toml..." -ForegroundColor Yellow
$cargoContent = Get-Content Cargo.toml -Raw
$versionMatch = [regex]::Match($cargoContent, '(?ms)^\[package\][^\[]*?^version\s*=\s*"([^"]+)"')
$CARGO_VERSION = $versionMatch.Groups[1].Value

if ([string]::IsNullOrWhiteSpace($CARGO_VERSION)) {
    Write-Host "❌ Failed to extract version from Cargo.toml" -ForegroundColor Red
    exit 1
}
Write-Host "✅ Cargo.toml version: $CARGO_VERSION" -ForegroundColor Green
Write-Host ""

# Step 2: Validate CHANGELOG.md has entry
Write-Host "[2/5] Checking CHANGELOG.md for version entry..." -ForegroundColor Yellow
$changelogContent = Get-Content CHANGELOG.md -Raw
if ($changelogContent -match "\## \[$CARGO_VERSION\]") {
    Write-Host "✅ CHANGELOG.md has entry for v$CARGO_VERSION" -ForegroundColor Green
} else {
    Write-Host "❌ CHANGELOG.md missing entry for v$CARGO_VERSION" -ForegroundColor Red
    Write-Host "   Expected: ## [$CARGO_VERSION] - YYYY-MM-DD" -ForegroundColor Red
    exit 1
}
Write-Host ""

# Step 3: Validate CHANGELOG_jp.md has entry
Write-Host "[3/5] Checking CHANGELOG_jp.md for version entry..." -ForegroundColor Yellow
$changelogJpContent = Get-Content CHANGELOG_jp.md -Raw
if ($changelogJpContent -match "\## \[$CARGO_VERSION\]") {
    Write-Host "✅ CHANGELOG_jp.md has entry for v$CARGO_VERSION" -ForegroundColor Green
} else {
    Write-Host "❌ CHANGELOG_jp.md missing entry for v$CARGO_VERSION" -ForegroundColor Red
    Write-Host "   Expected: ## [$CARGO_VERSION] - YYYY-MM-DD" -ForegroundColor Red
    exit 1
}
Write-Host ""

# Step 4: Validate SBOM version (if exists)
Write-Host "[4/5] Checking SBOM version consistency..." -ForegroundColor Yellow
if (Test-Path "sbom/fds.cdx.json") {
    try {
        $sbomJson = Get-Content sbom/fds.cdx.json | ConvertFrom-Json
        $SBOM_VERSION = $sbomJson.metadata.component.version
        
        if ([string]::IsNullOrWhiteSpace($SBOM_VERSION)) {
            Write-Host "⚠️  SBOM exists but version not found (will regenerate on next push)" -ForegroundColor Yellow
        } elseif ($SBOM_VERSION -eq $CARGO_VERSION) {
            Write-Host "✅ SBOM version matches: $SBOM_VERSION" -ForegroundColor Green
        } else {
            Write-Host "⚠️  SBOM is $SBOM_VERSION, Cargo.toml is $CARGO_VERSION (the SBOM workflow updates it after push; tag only after that)" -ForegroundColor Yellow
        }
    } catch {
        Write-Host "⚠️  SBOM file exists but is not valid JSON (will regenerate on next push)" -ForegroundColor Yellow
    }
} else {
    Write-Host "⚠️  SBOM not yet generated (will be created by workflow)" -ForegroundColor Yellow
}
Write-Host ""

# Step 5: Validate Semantic Versioning
Write-Host "[5/5] Validating Semantic Versioning format..." -ForegroundColor Yellow
if ($CARGO_VERSION -match "^[0-9]+\.[0-9]+\.[0-9]+$") {
    Write-Host "✅ Version format is valid: $CARGO_VERSION (X.Y.Z)" -ForegroundColor Green
} else {
    Write-Host "❌ Invalid version format: $CARGO_VERSION" -ForegroundColor Red
    Write-Host "   Expected: X.Y.Z (e.g., 0.8.0, 1.0.0)" -ForegroundColor Red
    exit 1
}
Write-Host ""

# Final Summary
Write-Host "════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host "✅ All Local Validation Checks Passed!" -ForegroundColor Green
Write-Host "════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host ""
Write-Host "📦 Current Version: $CARGO_VERSION" -ForegroundColor Cyan
Write-Host ""
Write-Host "Next Steps:" -ForegroundColor Cyan
Write-Host "  1. Run tests locally (optional):" -ForegroundColor Gray
Write-Host "     `$ cargo test" -ForegroundColor Gray
Write-Host "  2. Commit the new workflow files:" -ForegroundColor Gray
Write-Host "     `$ git add .github/workflows/pre-release-check.yml" -ForegroundColor Gray
Write-Host "     `$ git add .github/workflows/release.yml" -ForegroundColor Gray
Write-Host "     `$ git add RELEASE_PLAN.md PACKAGE_MANAGER_GUIDE.md" -ForegroundColor Gray
Write-Host "     `$ git commit -m 'Add automated release workflow'" -ForegroundColor Gray
Write-Host "  3. Push to main:" -ForegroundColor Gray
Write-Host "     `$ git push origin main" -ForegroundColor Gray
Write-Host ""
Write-Host "Then CI will run pre-release-check.yml automatically." -ForegroundColor Gray
Write-Host "════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host ""
