#!/usr/bin/env bash
# Local version validation script for fds release workflow
# This script verifies all version consistency checks before pushing

set -e

echo ""
echo "════════════════════════════════════════════════════════"
echo "🔍 fds Release Validation Script"
echo "════════════════════════════════════════════════════════"
echo ""

# Step 1: Extract Cargo.toml version
echo "[1/5] Extracting version from Cargo.toml..."
CARGO_VERSION=$(grep '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')
if [ -z "$CARGO_VERSION" ]; then
  echo "❌ Failed to extract version from Cargo.toml"
  exit 1
fi
echo "✅ Cargo.toml version: $CARGO_VERSION"
echo ""

# Step 2: Validate CHANGELOG.md has entry
echo "[2/5] Checking CHANGELOG.md for version entry..."
if grep -q "^## \[$CARGO_VERSION\]" CHANGELOG.md; then
  echo "✅ CHANGELOG.md has entry for v$CARGO_VERSION"
else
  echo "❌ CHANGELOG.md missing entry for v$CARGO_VERSION"
  echo "   Expected: ## [$CARGO_VERSION] - YYYY-MM-DD"
  exit 1
fi
echo ""

# Step 3: Validate CHANGELOG_jp.md has entry
echo "[3/5] Checking CHANGELOG_jp.md for version entry..."
if grep -q "^## \[$CARGO_VERSION\]" CHANGELOG_jp.md; then
  echo "✅ CHANGELOG_jp.md has entry for v$CARGO_VERSION"
else
  echo "❌ CHANGELOG_jp.md missing entry for v$CARGO_VERSION"
  echo "   Expected: ## [$CARGO_VERSION] - YYYY-MM-DD"
  exit 1
fi
echo ""

# Step 4: Validate SBOM version (if exists)
echo "[4/5] Checking SBOM version consistency..."
if [ -f "sbom/fds.cdx.json" ]; then
  SBOM_VERSION=$(jq -r '.metadata.component.version' sbom/fds.cdx.json 2>/dev/null || echo "")
  if [ -z "$SBOM_VERSION" ]; then
    echo "⚠️  SBOM exists but version not found (will regenerate on next push)"
  elif [ "$SBOM_VERSION" = "$CARGO_VERSION" ]; then
    echo "✅ SBOM version matches: $SBOM_VERSION"
  else
    echo "❌ SBOM version mismatch: Cargo.toml=$CARGO_VERSION, SBOM=$SBOM_VERSION"
    exit 1
  fi
else
  echo "⚠️  SBOM not yet generated (will be created by workflow)"
fi
echo ""

# Step 5: Validate Semantic Versioning
echo "[5/5] Validating Semantic Versioning format..."
if [[ "$CARGO_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "✅ Version format is valid: $CARGO_VERSION (X.Y.Z)"
else
  echo "❌ Invalid version format: $CARGO_VERSION"
  echo "   Expected: X.Y.Z (e.g., 0.8.0, 1.0.0)"
  exit 1
fi
echo ""

# Final Summary
echo "════════════════════════════════════════════════════════"
echo "✅ All Local Validation Checks Passed!"
echo "════════════════════════════════════════════════════════"
echo ""
echo "📦 Current Version: $CARGO_VERSION"
echo ""
echo "Next Steps:"
echo "  1. Run tests locally (optional):"
echo "     $ cargo test"
echo "  2. Commit the new workflow files:"
echo "     $ git add .github/workflows/pre-release-check.yml"
echo "     $ git add .github/workflows/release.yml"
echo "     $ git add RELEASE_PLAN.md PACKAGE_MANAGER_GUIDE.md"
echo "     $ git commit -m 'Add automated release workflow'"
echo "  3. Push to main:"
echo "     $ git push origin main"
echo ""
echo "Then CI will run pre-release-check.yml automatically."
echo "════════════════════════════════════════════════════════"
echo ""
