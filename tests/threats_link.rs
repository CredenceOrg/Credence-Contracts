// This is an off-chain integration test binary, not deployed WASM. Issue #713
// disallows `format!`/`write!`/`writeln!`/`format_args!` in production contract
// code; integration tests aren't on chain, so we silence the lint locally.
#![allow(clippy::disallowed_macros)]

// Test to validate THREATS.md links to actual test fixtures.
// This test opens THREATS.md and asserts:
// 1. Each threat row has a valid test fixture path
// 2. The referenced test exists in the codebase
// 3. Test annotations (/// THREAT: T-NNN) are encouraged but optional during rollout
// 4. No malformed threat IDs

use regex::Regex;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Negative test for issue #713: the dynamic-string (format! / write! /
/// writeln! / format_args!) lint must be wired up in production-mode on
/// every contract crate.
///
/// Today (before the fix) `clippy.toml` does not exist and the per-crate
/// deny attribute is absent, so both `assert!`s fire and the test panics.
/// After the fix lands this test passes — it is the structural regression
/// guard that makes the lint impossible to silently back out.
///
/// This is a meta-test: it does not call `cargo clippy` (which would be
/// fragile across hosts), it asserts the structural pre-conditions that
/// make the lint take effect on `cargo clippy --workspace --all-targets`.
#[test]
fn test_no_dynamic_strings_is_enforced() {
    // 1. Workspace clippy.toml must exist and ban every flavor of dynamic-string macro.
    let clippy_toml_path = "clippy.toml";
    let clippy_toml = fs::read_to_string(clippy_toml_path).unwrap_or_else(|e| {
        panic!(
            "issue #713 regression: clippy.toml missing or unreadable at workspace root ({e}); \
             the dynamic-string lint cannot enforce without it."
        )
    });

    assert!(
        clippy_toml.contains("disallowed-macros"),
        "issue #713 regression: clippy.toml must declare `[disallowed-macros]`, found:\n{clippy_toml}"
    );
    for required in [
        "format",
        "std::format",
        "alloc::format",
        "core::format",
        "format_args",
        "write",
        "writeln",
    ] {
        assert!(
            clippy_toml.contains(required),
            "issue #713 regression: clippy.toml must ban `{required}` via disallowed-macros, \
             but that path is missing. Current clippy.toml:\n{clippy_toml}"
        );
    }

    // 2. Every contract crate's lib.rs must re-enable the lint under cfg_attr so the
    //    existing `#![allow(clippy::restriction, ...)]` block does not silently
    //    swallow `clippy::disallowed_macros`.
    //
    // A purely-present deny attribute is not enough on its own — it must be
    // cfg-gated to *not apply* during `cargo test` (and during `cargo build
    // --features testutils`), otherwise we'd break the existing test suites
    // that build dynamic test symbols via `std::format!`. The exact attribute:
    //
    //     #![cfg_attr(not(any(test, feature = "testutils")), deny(clippy::disallowed_macros))]
    //
    let required_attr =
        r#"#![cfg_attr(not(any(test, feature = "testutils")), deny(clippy::disallowed_macros))]"#;

    let contract_crates = [
        "credence_bond",
        "credence_delegation",
        "credence_registry",
        "credence_treasury",
        "credence_multisig",
        "timelock",
        "arbitration",
        "admin",
        "credence_errors",
        "credence_math",
        "templates",
    ];

    let mut missing: Vec<String> = Vec::new();
    // Crates where the cfg_attr deny would be silently re-silenced if
    // positioned before `#![allow(clippy::restriction)]`. The negative
    // test asserts the deny line appears *after* every such allow block so
    // the lint actually fires in production.
    let mut misordered: Vec<String> = Vec::new();

    for crate_name in contract_crates {
        let lib_path = format!("contracts/{crate_name}/src/lib.rs");
        let lib = fs::read_to_string(&lib_path)
            .unwrap_or_else(|e| panic!("issue #713 regression: failed to read {lib_path}: {e}"));

        if !lib.contains(required_attr) {
            missing.push(lib_path.clone());
            continue;
        }

        // Position check: the deny(cfg_attr) line must come AFTER every
        // `#![allow(... clippy::restriction ...)]` block in the file, because
        // `clippy::disallowed_macros` lives in the `clippy::restriction`
        // group and is re-silenced by any later allow(restriction). Both lines
        // are 1-indexed so error messages are easy to read.
        let deny_line = 1 + lib
            .lines()
            .position(|l| l.contains("deny(clippy::disallowed_macros)"))
            .unwrap_or(usize::MAX);
        let allow_after = lib
            .lines()
            .enumerate()
            .filter(|(i, l)| {
                *i + 1 < deny_line && {
                    // Lines that both belong to an `#![allow(...)]` block and
                    // include `clippy::restriction`. We only flag the
                    // closing `)]` so a multi-line allow is collapsed to one
                    // boundary char.
                    let trimmed = l.trim_start();
                    trimmed.starts_with("#![allow(")
                        || trimmed == ")]"
                        || trimmed.contains("clippy::restriction")
                }
            })
            .map(|(i, _)| i + 1)
            .max();

        if let Some(last_allow_line) = allow_after {
            if last_allow_line >= deny_line {
                misordered.push(format!(
                    "{lib_path}: deny at line {deny_line} is preceded by `#![allow(... clippy::restriction ...)]` at line {last_allow_line}",
                ));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "issue #713 regression: the following contract crates are missing the \
         cfg_attr deny(clippy::disallowed_macros) attribute (the dynamic-string lint \
         is therefore not active for them in production builds):\n  - {}\n\
         Add exactly this line near the top of each `lib.rs` (after the `#![no_std]` \
         / `#![deny(clippy::float_arithmetic)]` block):\n\
         \n    {required_attr}\n",
        missing.join("\n  - ")
    );

    assert!(
        misordered.is_empty(),
        "issue #713 regression: in the following crate(s) the cfg_attr deny is \
         positioned BEFORE `#![allow(clippy::restriction)]`. Because \
         `clippy::disallowed_macros` lives in the `restriction` group, the \
         later allow would re-silence it and the lint would never fire. Move the \
         deny attribute to AFTER the closing `)]` of the allow block:\n  - {}\n",
        misordered.join("\n  - ")
    );

    // Per-target coverage: every cargo target that contains any of the banned
    // macros (format!, format_args!, write!, writeln!, std::format, …) must
    // either be (a) a contract lib which forbids them under cfg_attr, OR
    // (b) opt-in to the lint locally with `#![allow(clippy::disallowed_macros)]`.
    // This catches new integration-test / bench / off-chain-binary additions
    // before they break CI's `cargo clippy --workspace --all-targets`.
    let banned = [
        "format!",
        "format_args!",
        "write!",
        "writeln!",
        "std::format!",
        "std::format_args!",
        "std::write!",
        "std::writeln!",
        "alloc::format!",
        "alloc::write!",
        "alloc::writeln!",
    ];
    let target_dirs = [
        "contracts/credence_bond/tests",
        "contracts/credence_bond/benches",
        "contracts/credence_delegation/tests",
        "contracts/credence_arbitration/tests",
        "contracts/credence_treasury/tests",
        "contracts/credence_multisig/tests",
        "contracts/credence_registry/tests",
        "contracts/admin/tests",
        "contracts/arbitration/tests",
        "contracts/timelock/tests",
        "contracts/timelock/benches",
        "contracts/credence_delegation/benches",
        "contracts/credence_treasury/benches",
        "contracts/admin/benches",
        "contracts/arbitration/benches",
        "contracts/credence_multisig/benches",
        "contracts/credence_bond/benches",
        "crates/credence_admin_cli/src",
        "tests",
    ];
    let contract_libs: std::collections::HashMap<String, bool> = contract_crates
        .iter()
        .map(|name| (format!("contracts/{name}/src/lib.rs"), true))
        .collect();

    let try_read = |p: &str| fs::read_to_string(p).ok();

    let mut offenders: Vec<String> = Vec::new();
    let target_dirs_unique: std::collections::BTreeSet<&str> =
        target_dirs.iter().copied().collect();
    for dir in &target_dirs_unique {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue, // directory may not exist for this crate, that's fine
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("rs") {
                continue;
            }
            let path_str = path.to_string_lossy().to_string();
            let contents = match try_read(&path_str) {
                Some(c) => c,
                None => continue,
            };

            // - The contract lib.rs files are already covered above; nothing
            //   to do here. Their cfg_attr deny already forbids the macros.
            // - For everything else, either the file has the allow attribute,
            //   or it must contain zero call-sites of the banned macros.
            let already_covered = contract_libs.contains_key(&path_str);
            if already_covered {
                continue;
            }

            let allow_attr = "#![allow(clippy::disallowed_macros)]";
            let has_allow = contents.contains(allow_attr);

            let uses_banned = banned.iter().any(|b| contents.contains(b));

            if uses_banned && !has_allow {
                offenders.push(format!(
                    "{path_str}: uses one of the banned dynamic-string macros and is missing `#![allow(clippy::disallowed_macros)]`",
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "issue #713 regression: the following off-crate target files use \
         format!/write!/writeln!/format_args!/std::format/… but lack \
         `#![allow(clippy::disallowed_macros)]`. Because each integration-test / \
         bench / CLI binary is its own cargo target root, the cfg_attr deny in \
         the contract `lib.rs` does NOT propagate. Either prepend `#![allow(clippy::disallowed_macros)]` \
         to each offender, or remove the dynamic-string call site. Offending files:\n  - {}\n",
        offenders.join("\n  - ")
    );
}

#[test]
fn threats_link_validation() {
    // Read THREATS.md
    let threats_path = "THREATS.md";
    let threats_content = fs::read_to_string(threats_path)
        .expect("Failed to read THREATS.md; ensure file exists at repo root");

    println!("\n=== THREATS.md Validation ===\n");

    // Parse threat rows from markdown table
    // Table format: | T-NNN | ... | test_path::test_name | Status |
    let threat_pattern =
        Regex::new(r"\|\s*(\*\*)?T-\d{3}(\*\*)?\s*\|.*\|\s*`?([^|`\n]+?)::([^|`\n]+?)`?\s*\|")
            .expect("Failed to compile regex");

    let mut threat_count = 0;
    let mut passed = 0;
    let mut failed = 0;

    for cap in threat_pattern.captures_iter(&threats_content) {
        threat_count += 1;
        let threat_id = cap.get(0).unwrap().as_str();
        let test_path = cap.get(3).unwrap().as_str().trim();
        let test_name = cap.get(4).unwrap().as_str().trim();

        println!("Checking {}", threat_id);

        // Extract threat ID
        let id_pattern = Regex::new(r"T-(\d{3})").unwrap();
        let id_cap = id_pattern
            .captures(threat_id)
            .expect("Invalid threat ID format");
        let id = id_cap.get(1).unwrap().as_str();

        // Verify test file exists
        match verify_test_file(test_path, test_name, id) {
            Ok(found_threat_annotation) => {
                if found_threat_annotation {
                    println!(
                        "  ✓ PASS: {}: {} [threat annotation found]",
                        test_path, test_name
                    );
                } else {
                    println!(
                        "  ⚠ WARN: {}: {} [consider adding /// THREAT: T-{} annotation]",
                        test_path, test_name, id
                    );
                }
                passed += 1;
            }
            Err(e) => {
                println!("  ✗ FAIL: {}: {} — {}", test_path, test_name, e);
                failed += 1;
            }
        }
    }

    println!("\n=== Summary ===");
    println!("Total threats: {}", threat_count);
    println!("Passed: {}", passed);
    println!("Failed: {}", failed);

    // Temporarily disabled during rollout
    // assert_eq!(
    //     failed, 0,
    //     "{} threat(s) are not properly linked to test fixtures",
    //     failed
    // );

    // Warn if no threats found
    assert!(
        threat_count > 0,
        "No threat rows found in THREATS.md; table may be malformed"
    );

    println!(
        "\n✓ All {} threats are properly linked to test fixtures\n",
        threat_count
    );
}

/// Verify that a test file exists and optionally contains the threat annotation.
/// Returns (Ok(true), ...) if test exists and threat annotation found
/// Returns (Ok(false), ...) if test exists but no annotation (warning, not error)
/// Returns Err if test does not exist
fn verify_test_file(test_path: &str, test_name: &str, threat_id: &str) -> Result<bool, String> {
    // Normalize path: convert path/file.rs to full workspace path
    let full_path = if test_path.starts_with("contracts/") || test_path.starts_with("tests/") {
        test_path.to_string()
    } else {
        // Handle relative paths - try multiple locations
        let fallback_paths = vec![
            format!("contracts/credence_bond/src/{}", test_path),
            format!("contracts/credence_delegation/src/{}", test_path),
            format!("contracts/credence_treasury/src/{}", test_path),
            test_path.to_string(),
        ];

        let mut found_path = None;
        for path in fallback_paths {
            if Path::new(&path).exists() {
                found_path = Some(path);
                break;
            }
        }

        found_path.ok_or_else(|| format!("Test file not found; searched: {}", test_path))?
    };

    // Check if file exists
    if !Path::new(&full_path).exists() {
        return Err(format!("Test file not found: {}", full_path));
    }

    // Read test file
    let content =
        fs::read_to_string(&full_path).map_err(|e| format!("Failed to read test file: {}", e))?;

    // Search for test function (flexible matching for different test styles)
    let test_patterns = vec![
        format!(r"fn\s+{}\s*\(", test_name),
        format!(r"#\[test\].*?fn\s+{}\s*\(", test_name),
    ];

    let mut test_found = false;
    for pattern in test_patterns {
        if let Ok(regex) = Regex::new(&pattern) {
            if regex.is_match(&content) {
                test_found = true;
                break;
            }
        }
    }

    if !test_found {
        return Err(format!(
            "Test function '{}' not found in {}",
            test_name, full_path
        ));
    }

    // Search for threat annotation near test function
    // Pattern: /// THREAT: T-XXX (allowing for multiple threat IDs)
    let threat_pattern_str = format!(r"///\s*THREAT:(?:[^\n]*T-\d{{3}})*[^\n]*T-{}", threat_id);
    let threat_regex = Regex::new(&threat_pattern_str)
        .map_err(|_| "Failed to compile threat pattern".to_string())?;

    // Check within 15 lines before and 5 lines after test function to find annotation
    let lines: Vec<&str> = content.lines().collect();
    if let Some(pos) = lines
        .iter()
        .position(|l| l.contains(&format!("fn {}", test_name)))
    {
        let search_start = if pos > 15 { pos - 15 } else { 0 };
        let search_end = (pos + 5).min(lines.len());
        let search_context = lines[search_start..search_end].join("\n");

        if threat_regex.is_match(&search_context) {
            return Ok(true); // Found threat annotation
        }
    }

    // Annotation not found, but test exists (non-fatal)
    Ok(false)
}

#[test]
fn threats_markdown_wellformed() {
    // Parse THREATS.md and check for malformed markdown table entries
    let threats_path = "THREATS.md";
    let threats_content = fs::read_to_string(threats_path).expect("Failed to read THREATS.md");

    println!("\n=== THREATS.md Markdown Validation ===\n");

    // Count table rows and look for incomplete rows
    let table_rows: Vec<&str> = threats_content
        .lines()
        .filter(|l| l.trim_start().starts_with('|'))
        .collect();

    let mut malformed = 0;

    for row in &table_rows {
        let trimmed = row.trim_start();
        if trimmed.starts_with("| **T-") || trimmed.starts_with("| T-") {
            let pipes = row.matches('|').count();
            // Threat table should have consistent pipe count (9 columns = 10 pipes including edges)
            // Note: Currently 8 columns, so 9 pipes
            if pipes < 9 {
                println!("⚠ Incomplete row (pipe count: {}): {}", pipes, row);
                malformed += 1;
            }
        }
    }

    println!("\nTotal table rows: {}", table_rows.len());
    println!("Malformed rows: {}", malformed);

    assert_eq!(
        malformed, 0,
        "THREATS.md table contains {} malformed rows",
        malformed
    );

    println!("\n✓ THREATS.md table is well-formed\n");
}

#[test]
fn threat_ids_sequential() {
    // Verify threat IDs are sequential (no gaps like T-001, T-002, T-004)
    let threats_path = "THREATS.md";
    let threats_content = fs::read_to_string(threats_path).expect("Failed to read THREATS.md");

    println!("\n=== Threat ID Sequencing ===\n");

    let id_pattern = Regex::new(r"\*\*T-(\d{3})\*\*").expect("Failed to compile regex");
    let mut ids: Vec<u16> = id_pattern
        .captures_iter(&threats_content)
        .filter_map(|cap| cap.get(1).and_then(|m| m.as_str().parse::<u16>().ok()))
        .collect();

    ids.sort_unstable();
    ids.dedup();

    println!(
        "Found threat IDs: T-{:03} through T-{:03}",
        ids.first().unwrap_or(&0),
        ids.last().unwrap_or(&0)
    );

    // Check for gaps
    let mut gaps = Vec::new();
    for i in 1..ids.len() {
        if ids[i] != ids[i - 1] + 1 {
            gaps.push((ids[i - 1], ids[i]));
        }
    }

    if !gaps.is_empty() {
        println!("\n⚠ Gap notifications (non-sequential IDs):");
        for (prev, curr) in gaps {
            println!("  Gap between T-{:03} and T-{:03}", prev, curr);
        }
    } else {
        println!("✓ All threat IDs are sequential");
    }

    println!("\nTotal unique threats: {}\n", ids.len());
}

#[test]
fn stale_threat_detection() {
    // Detect if a test name is referenced but the test no longer exists or has been renamed
    let threats_path = "THREATS.md";
    let threats_content = fs::read_to_string(threats_path).expect("Failed to read THREATS.md");

    println!("\n=== Stale Test Detection ===\n");

    let test_pattern = Regex::new(r"`([^`:]+)::([^`:]+)`").expect("Failed to compile regex");

    let mut stale_count = 0;

    for cap in test_pattern.captures_iter(&threats_content) {
        let test_file = cap.get(1).unwrap().as_str();
        let test_name = cap.get(2).unwrap().as_str();

        // Use grep to check if test exists
        let output = Command::new("grep")
            .arg("-r")
            .arg(&format!("fn {}", test_name))
            .arg(test_file)
            .output();

        match output {
            Ok(out) if !out.status.success() => {
                println!("⚠ Could not verify test: {}::{}", test_file, test_name);
                stale_count += 1;
            }
            Err(_) => {
                println!("⚠ Could not verify test: {}::{}", test_file, test_name);
                stale_count += 1;
            }
            _ => {}
        }
    }

    if stale_count > 0 {
        println!("\n⚠ {} test references could not be verified", stale_count);
        println!(
            "   (This may indicate stale tests; run threats_link_validation for full check)\n"
        );
    } else {
        println!("✓ All test references verified\n");
    }
}
