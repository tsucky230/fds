# Security Scan Report

**Date**: 2026-11-01  
**Tool**: Grype v0.121.0  
**SBOM**: sbom/fds.cdx.json  
**Status**: ⚠️ VULNERABILITIES DETECTED

---

## Executive Summary

The FDS project dependency scan using Grype revealed **5 vulnerabilities**, all in the `zip` crate (v8.6.0):
- **1 Critical** severity
- **3 High** severity  
- **1 Medium** severity

**All vulnerabilities can be resolved by upgrading the `zip` crate to v9.0.5 or later.**

---

## Detailed Findings

### Affected Package

| Field | Value |
|-------|-------|
| Name | zip |
| Current Version | 8.6.0 |
| Recommended Version | 9.0.5 or later |
| Type | rust-crate |

### Vulnerability Details

#### 1. GHSA-m665-v7pm-7grj (CRITICAL)
- **Severity**: Critical  
- **Fixed In**: 9.0.5
- **EPSS Score**: 0.3% (25th percentile)
- **Risk Level**: 0.3
- **Impact**: Archive path traversal vulnerability in zip crate

#### 2. GHSA-q7v9-x93c-xqv3 (HIGH)
- **Severity**: High
- **Fixed In**: 9.0.0
- **EPSS Score**: 0.5% (39th percentile)
- **Risk Level**: 0.3

#### 3. GHSA-hjqx-j4rw-g7xv (HIGH)
- **Severity**: High
- **Fixed In**: 9.0.0
- **EPSS Score**: 0.4% (37th percentile)
- **Risk Level**: 0.3

#### 4. GHSA-9hj8-73fh-9963 (HIGH)
- **Severity**: High
- **Fixed In**: 9.0.0
- **EPSS Score**: 0.4% (28th percentile)
- **Risk Level**: 0.3

#### 5. GHSA-8hxf-qr8p-62x4 (MEDIUM)
- **Severity**: Medium
- **Fixed In**: 9.0.0
- **EPSS Score**: 0.2% (4th percentile)
- **Risk Level**: < 0.1

---

## Remediation

### Recommended Action

Update `Cargo.toml`:

```toml
# Before
zip = "8.6.0"

# After
zip = "9.0.5"
```

### Steps to Fix

1. **Update Cargo.toml**:
   ```bash
   # Manually edit Cargo.toml or use:
   cargo update zip -p zip --precise 9.0.5
   ```

2. **Verify the update**:
   ```bash
   cargo tree | grep zip
   # Should show: zip v9.0.5 or later
   ```

3. **Re-run security scan**:
   ```bash
   grype sbom:sbom/fds.cdx.json
   # Should show 0 vulnerabilities
   ```

4. **Commit changes**:
   ```bash
   git add Cargo.toml Cargo.lock
   git commit -m "chore: upgrade zip crate to 9.0.5 for security fixes"
   ```

---

## Compliance Status

### GPL-3.0-only Requirements

| Requirement | Status |
|-------------|--------|
| LICENSE file | ✓ Present |
| Source code available | ✓ GitHub |
| Dependencies listed | ✓ SBOM + NOTICE |
| License info per dependency | ✓ 314/314 documented |
| No license violations | ✓ Verified |
| Security scan performed | ✓ Grype scan complete |

### Vulnerability Impact on Compliance

The detected vulnerabilities do **not** affect GPL-3.0-only compliance directly. However, they should be addressed as part of responsible software maintenance.

---

## Next Steps

1. ✅ Vulnerability detection complete (Grype)
2. ⏳ Update zip crate to 9.0.5
3. ⏳ Re-run Grype scan
4. ⏳ Regenerate SBOM (automatic via CI)
5. ⏳ Document in CHANGELOG_jp.md
6. ⏳ Tag security release

---

## References

- **Grype**: https://github.com/anchore/grype
- **SBOM**: [sbom/fds.cdx.json](sbom/fds.cdx.json)
- **GitHub Security Advisories**: https://github.com/advisories
  - [GHSA-m665-v7pm-7grj](https://github.com/advisories/GHSA-m665-v7pm-7grj)
  - [GHSA-q7v9-x93c-xqv3](https://github.com/advisories/GHSA-q7v9-x93c-xqv3)
  - [GHSA-hjqx-j4rw-g7xv](https://github.com/advisories/GHSA-hjqx-j4rw-g7xv)
  - [GHSA-9hj8-73fh-9963](https://github.com/advisories/GHSA-9hj8-73fh-9963)
  - [GHSA-8hxf-qr8p-62x4](https://github.com/advisories/GHSA-8hxf-qr8p-62x4)

---

**Report Generated**: 2026-11-01  
**Scan Duration**: < 1 minute
