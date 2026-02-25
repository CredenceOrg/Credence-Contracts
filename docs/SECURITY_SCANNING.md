# Security Scanning

Automated security analysis pipeline for Credence smart contracts. This document covers the security scanning tools, how to run them locally, interpret results, and manage findings.

## Overview

The security scanning pipeline runs automatically on every push and pull request to `main` and `develop` branches. It consists of three complementary tools:

1. **cargo-audit** - Scans dependencies for known security vulnerabilities
2. **cargo-clippy** - Static analysis with security-focused lints
3. **cargo-geiger** - Detects unsafe code blocks in contracts

## Pipeline Behavior

### Failure Conditions

The pipeline will **FAIL** on:
- Critical severity vulnerabilities in dependencies (cargo-audit)
- Security lint violations (clippy with `-D warnings`)

### Warning Conditions

The pipeline will **PASS WITH WARNINGS** on:
- Medium/low severity vulnerabilities in dependencies
- Unsafe code detected in contracts (informational only)

## Running Scanners Locally

### Prerequisites

Install the required tools:

```bash
# Install cargo-audit (version pinned to match CI)
cargo install cargo-audit --version 0.22.0 --locked

# Install cargo-geiger (version pinned to match CI)
cargo install cargo-geiger --version 0.12.0 --locked

# Clippy is included with rustup
rustup component add clippy
```

### cargo-audit: Dependency Vulnerabilities

Scan all dependencies for known vulnerabilities:

```bash
# Basic scan
cargo audit

# Generate JSON report
cargo audit --json > audit-report.json

# Check only for critical vulnerabilities
cargo audit --deny warnings
```

**What it checks:**
- Known CVEs in direct and transitive dependencies
- Unmaintained crates
- Yanked crate versions
- Supports CVSS 3.x and 4.0 scoring (requires cargo-audit 0.22.0+)

### cargo-clippy: Security Lints

Run static analysis with security-focused lints:

```bash
# Run the same checks as CI
cargo clippy --all-targets -- \
  -W clippy::integer_arithmetic \
  -W clippy::unwrap_used \
  -W clippy::expect_used \
  -W clippy::panic \
  -W clippy::todo \
  -W clippy::unimplemented \
  -W clippy::indexing_slicing \
  -W clippy::cast_possible_truncation \
  -W clippy::cast_sign_loss \
  -D warnings
```

**What it checks:**
- Integer overflow/underflow risks
- Panic-inducing operations (unwrap, expect, panic!)
- Unsafe type casting
- Array indexing without bounds checks
- Incomplete code markers (todo!, unimplemented!)

### cargo-geiger: Unsafe Code Detection

Detect unsafe code blocks in contracts:

```bash
# Scan contracts directory
cd contracts
cargo geiger

# Generate markdown report
cargo geiger --output-format GitHubMarkdown > geiger-report.md

# Generate JSON for programmatic analysis
cargo geiger --output-format Json > geiger-report.json
```

**What it checks:**
- Unsafe functions, expressions, implementations
- Unsafe traits and methods
- Unsafe code in dependencies vs. your code

## Interpreting Reports

### cargo-audit Report

```json
{
  "vulnerabilities": {
    "list": [
      {
        "advisory": {
          "id": "RUSTSEC-2024-XXXX",
          "severity": "critical",
          "title": "Vulnerability description",
          "description": "Detailed explanation"
        },
        "versions": {
          "patched": [">=1.2.3"]
        }
      }
    ]
  }
}
```

**Severity levels:**
- `critical` - Immediate action required, pipeline fails
- `high` - Review and plan remediation
- `medium` - Monitor and update when convenient
- `low` - Informational

**Action items:**
- Update affected dependencies to patched versions
- If no patch available, consider alternatives or mitigations
- Document accepted risks in `audit.toml` if needed

### Clippy Report

Clippy outputs warnings/errors with file location and explanation:

```
warning: used `unwrap()` on a `Result` value
  --> contracts/credence_bond/src/lib.rs:42:18
   |
42 |     let value = result.unwrap();
   |                  ^^^^^^^^^^^^^^
   |
   = help: consider using `expect()` with a meaningful message or proper error handling
```

**Common security lints:**
- `integer_arithmetic` - Potential overflow/underflow
- `unwrap_used` - Can panic on None/Err
- `indexing_slicing` - Can panic on out-of-bounds
- `cast_possible_truncation` - Data loss in type conversion

**Action items:**
- Replace `unwrap()` with proper error handling
- Use checked arithmetic operations
- Add bounds checks before indexing
- Use safe type conversions

### cargo-geiger Report

```
Metric output format: x/y
    x = unsafe code used by the build
    y = total unsafe code found in the crate

Functions  Expressions  Impls  Traits  Methods  Dependency

0/0        0/0          0/0    0/0     0/0      credence_bond
2/2        5/5          0/0    0/0     1/1      soroban-sdk
```

**Interpretation:**
- First number: unsafe code actually used
- Second number: total unsafe code available
- Focus on your contract crates (credence_*)
- Dependencies may have unsafe code (expected for low-level libs)

**Action items:**
- Minimize unsafe code in contracts
- Document why unsafe is necessary if used
- Prefer safe abstractions from dependencies

## Triaging and Resolving Findings

### 1. Review the Finding

Download reports from GitHub Actions artifacts:
- Go to the workflow run
- Scroll to "Artifacts" section
- Download relevant reports

### 2. Assess Severity and Impact

**Critical/High:**
- Does it affect contract logic?
- Can it be exploited?
- Is there a patch available?

**Medium/Low:**
- What's the attack surface?
- Is the vulnerable code path reachable?
- What's the remediation timeline?

### 3. Remediation Options

**Option A: Update Dependencies**
```bash
# Update specific crate
cargo update -p <crate-name>

# Update all dependencies
cargo update

# Test after update
cargo test
```

**Option B: Replace Dependency**
```toml
# In Cargo.toml, replace vulnerable crate
[dependencies]
# old-crate = "1.0"
new-crate = "2.0"
```

**Option C: Accept Risk (with documentation)**

Create `audit.toml` in workspace root:

```toml
[advisories]
ignore = [
    "RUSTSEC-2024-XXXX",  # Reason: Not exploitable in our context
]
```

**Option D: Fix Code Issues**

For clippy findings, refactor code:

```rust
// Before (unsafe)
let value = result.unwrap();

// After (safe)
let value = result.expect("Failed to get value: this should never happen");
// Or better:
let value = result.map_err(|e| Error::InvalidValue)?;
```

### 4. Verify the Fix

```bash
# Run all security scans locally
cargo audit
cargo clippy --all-targets -- -D warnings
cd contracts && cargo geiger

# Run tests
cargo test
```

### 5. Document in PR

Include in your PR description:
- Which finding was addressed
- How it was fixed
- Why the approach was chosen
- Test results

## Updating Scanner Versions

Scanner versions are pinned in `.github/workflows/security.yml` to ensure deterministic results.

### When to Update

- New scanner version with important features
- Security fix in the scanner itself
- Compatibility with new Rust version

### How to Update

1. Test locally first:

```bash
# Install new version
cargo install cargo-audit --version 0.21.0 --locked

# Run full scan
cargo audit
```

2. Update workflow file:

```yaml
- name: Install cargo-audit
  run: cargo install cargo-audit --version 0.21.0 --locked
```

3. Update this documentation with new version

4. Test in CI by pushing to a feature branch

### Version History

| Tool | Current Version | Last Updated |
|------|----------------|--------------|
| cargo-audit | 0.22.0 | 2024-02-23 |
| cargo-geiger | 0.12.0 | 2024-02-23 |
| clippy | stable | (follows Rust toolchain) |

## Adjusting Severity Thresholds

### cargo-audit Thresholds

Edit the "Check for critical vulnerabilities" step in `security.yml`:

```yaml
# Current: Fail only on critical
CRITICAL_COUNT=$(jq '[.vulnerabilities.list[] | select(.advisory.severity == "critical")] | length' audit-report.json)

# Option: Fail on critical OR high
HIGH_CRITICAL_COUNT=$(jq '[.vulnerabilities.list[] | select(.advisory.severity == "critical" or .advisory.severity == "high")] | length' audit-report.json)
```

### Clippy Thresholds

Current configuration uses `-D warnings` which treats all warnings as errors.

To allow specific lints:

```yaml
# Change from -W (warn) to -A (allow)
-A clippy::todo  # Allow TODO markers
```

To add more strict lints:

```yaml
# Add additional security lints
-W clippy::mem_forget
-W clippy::print_stdout
-W clippy::exit
```

## CI Artifacts

Every security scan run produces downloadable artifacts:

### Available Artifacts

1. **cargo-audit-report** (JSON)
   - Vulnerability details
   - Affected versions
   - Patch information

2. **clippy-security-report** (HTML + JSON + TXT)
   - HTML: Human-readable report
   - JSON: Machine-parseable output
   - TXT: Summary statistics

3. **cargo-geiger-report** (Markdown)
   - Unsafe code metrics
   - Per-crate breakdown

4. **security-summary** (Markdown)
   - Overall scan status
   - Quick reference

### Downloading Artifacts

1. Navigate to Actions tab in GitHub
2. Click on the workflow run
3. Scroll to "Artifacts" section at bottom
4. Click artifact name to download ZIP

Artifacts are retained for 30 days.

## Manual Workflow Trigger

You can trigger security scans manually without pushing code:

1. Go to Actions tab
2. Select "Security Scanning" workflow
3. Click "Run workflow" button
4. Select branch
5. Click "Run workflow"

Useful for:
- Testing scanner configuration changes
- Periodic security audits
- Generating reports for compliance

## First-Time Setup

### No Additional Setup Required

The security scanning workflow is fully automated and requires no secrets, tokens, or manual configuration.

### Optional: Advisory Database

cargo-audit automatically downloads the RustSec advisory database. To pre-populate locally:

```bash
cargo audit fetch
```

### Optional: Custom Audit Configuration

Create `audit.toml` in workspace root to customize behavior:

```toml
[advisories]
# Ignore specific advisories (with justification)
ignore = []

# Fail on informational advisories
informational_warnings = ["unmaintained"]

[yanked]
# Fail on yanked crates
enabled = true
```

## Troubleshooting

### Scanner Installation Fails

```bash
# Clear cargo cache
rm -rf ~/.cargo/registry
rm -rf ~/.cargo/git

# Reinstall
cargo install cargo-audit --version 0.20.0 --locked --force
```

### False Positives

If a vulnerability doesn't apply to your usage:

1. Document why in `audit.toml`
2. Add to ignore list with comment
3. Link to issue/discussion if available

### CVSS Version Errors

If you see errors like "unsupported CVSS version: 4.0":

```
error parsing RUSTSEC-2026-XXXX.md: unsupported CVSS version: 4.0
```

This means your cargo-audit version is too old. The RustSec advisory database now includes CVSS 4.0 scores (introduced November 2023).

**Solution:** Upgrade to cargo-audit 0.22.0 or later:

```bash
cargo install cargo-audit --version 0.22.0 --locked --force
```

**Why this happens:**
- CVSS 4.0 was released in November 2023
- RustSec advisories started using CVSS 4.0 in 2026
- Older cargo-audit versions only support CVSS 3.x
- This is a tooling compatibility issue, not a security vulnerability in your project

### CI Cache Issues

If CI shows stale results:

1. Go to Actions tab
2. Click "Caches" in left sidebar
3. Delete relevant caches
4. Re-run workflow

## Best Practices

1. **Run locally before pushing** - Catch issues early
2. **Review all findings** - Don't blindly ignore warnings
3. **Update regularly** - Keep dependencies current
4. **Document exceptions** - Explain why risks are accepted
5. **Monitor advisories** - Subscribe to RustSec announcements
6. **Test after fixes** - Ensure functionality isn't broken
7. **Scope scans to contracts** - Focus on critical code

## Resources

- [RustSec Advisory Database](https://rustsec.org/)
- [Clippy Lint Documentation](https://rust-lang.github.io/rust-clippy/master/)
- [cargo-audit Documentation](https://github.com/rustsec/rustsec/tree/main/cargo-audit)
- [cargo-geiger Documentation](https://github.com/geiger-rs/cargo-geiger)
- [Soroban Security Best Practices](https://developers.stellar.org/docs/smart-contracts/security)

## Support

For questions or issues with security scanning:

1. Check this documentation
2. Review existing GitHub issues
3. Open a new issue with:
   - Scanner output
   - Steps to reproduce
   - Expected vs actual behavior
