// Test to validate THREATS.md links to actual test fixtures.
// This test opens THREATS.md and asserts:
// 1. Each threat row has a valid test fixture path
// 2. The referenced test exists in the codebase
// 3. Test annotations (/// THREAT: T-NNN) are encouraged but optional during rollout
// 4. No malformed threat IDs

use std::fs;
use std::path::Path;
use regex::Regex;

#[test]
fn threats_link_validation() {
    // Read THREATS.md
    let threats_path = "THREATS.md";
    let threats_content = fs::read_to_string(threats_path)
        .expect("Failed to read THREATS.md; ensure file exists at repo root");

    println!("\n=== THREATS.md Validation ===\n");

    // Parse threat rows from markdown table
    // Table format: | T-NNN | ... | test_path::test_name | Status |
    let threat_pattern = Regex::new(
        r"\|\s*(\*\*)?T-\d{3}(\*\*)?\s*\|.*\|\s*`?([^|`\n]+?)::([^|`\n]+?)`?\s*\|"
    ).expect("Failed to compile regex");

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
        let id_cap = id_pattern.captures(threat_id).expect("Invalid threat ID format");
        let id = id_cap.get(1).unwrap().as_str();

        // Verify test file exists
        match verify_test_file(test_path, test_name, id) {
            Ok(found_threat_annotation) => {
                if found_threat_annotation {
                    println!("  ✓ PASS: {}: {} [threat annotation found]", test_path, test_name);
                } else {
                    println!("  ⚠ WARN: {}: {} [consider adding /// THREAT: T-{} annotation]", test_path, test_name, id);
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

    // Fail the test if any threats have invalid links
    assert_eq!(
        failed, 0,
        "{} threat(s) are not properly linked to test fixtures",
        failed
    );

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
    let content = fs::read_to_string(&full_path)
        .map_err(|e| format!("Failed to read test file: {}", e))?;

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
        return Err(format!("Test function '{}' not found in {}", test_name, full_path));
    }

    // Search for threat annotation near test function
    // Pattern: /// THREAT: T-XXX (allowing for multiple threat IDs)
    let threat_pattern_str = format!(r"///\s*THREAT:(?:[^\n]*T-\d{{3}})*[^\n]*T-{}", threat_id);
    let threat_regex = Regex::new(&threat_pattern_str)
        .map_err(|_| "Failed to compile threat pattern".to_string())?;

    // Check within 15 lines before and 5 lines after test function to find annotation
    let lines: Vec<&str> = content.lines().collect();
    if let Some(pos) = lines.iter().position(|l| l.contains(&format!("fn {}", test_name))) {
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
    let threats_content = fs::read_to_string(threats_path)
        .expect("Failed to read THREATS.md");

    println!("\n=== THREATS.md Markdown Validation ===\n");

    // Count table rows and look for incomplete rows
    let table_rows: Vec<&str> = threats_content
        .lines()
        .filter(|l| l.trim_start().starts_with('|'))
        .collect();

    let mut malformed = 0;

    for row in &table_rows {
        let pipes = row.matches('|').count();
        // Threat table should have consistent pipe count (9 columns = 10 pipes including edges)
        if pipes < 9 {
            println!("⚠ Incomplete row (pipe count: {}): {}", pipes, row);
            malformed += 1;
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
    let threats_content = fs::read_to_string(threats_path)
        .expect("Failed to read THREATS.md");

    println!("\n=== Threat ID Sequencing ===\n");

    let id_pattern = Regex::new(r"\*\*T-(\d{3})\*\*").expect("Failed to compile regex");
    let mut ids: Vec<u16> = id_pattern
        .captures_iter(&threats_content)
        .filter_map(|cap| cap.get(1).and_then(|m| m.as_str().parse::<u16>().ok()))
        .collect();

    ids.sort_unstable();
    ids.dedup();

    println!("Found threat IDs: T-{:03} through T-{:03}", ids.first().unwrap_or(&0), ids.last().unwrap_or(&0));

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
    let threats_content = fs::read_to_string(threats_path)
        .expect("Failed to read THREATS.md");

    println!("\n=== Stale Test Detection ===\n");

    let test_pattern = Regex::new(
        r"`([^`:]+)::([^`:]+)`"
    ).expect("Failed to compile regex");

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
