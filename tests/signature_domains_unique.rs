// Workspace-level integration test asserting that every contract's
// SIGNATURE_DOMAIN constant is unique across the codebase.
//
// Without domain separation, a signature created for contract A could be
// replayed against contract B if both contracts share the same nonce namespace.
// Each contract must therefore carry a unique SIGNATURE_DOMAIN identifier.
//
// This test parses the source files to extract the constant values and
// verifies that:
//   1. Every "main" contract crate defines SIGNATURE_DOMAIN.
//   2. No two contracts share the same domain string.
#![allow(clippy::disallowed_macros)]

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

/// Contract crates that are expected to define a SIGNATURE_DOMAIN constant.
/// Utility crates (credence_errors, credence_math, templates) and the
/// admin CLI / testutils are excluded — they are not deployed contracts
/// that participate in cross-contract signature replay.
const CONTRACT_CRATES: &[&str] = &[
    "credence_bond",
    "credence_delegation",
    "credence_registry",
    "credence_treasury",
    "credence_multisig",
    "timelock",
    "arbitration",
    "admin",
];

/// If a contract's SIGNATURE_DOMAIN lives in a file other than `src/lib.rs`,
/// list the extra source paths to scan here.
const EXTRA_DOMAIN_FILES: &[(&str, &str)] =
    &[("credence_delegation", "contracts/credence_delegation/src/domain.rs")];

/// Extract the SIGNATURE_DOMAIN string value from source content.
/// Looks for: `const SIGNATURE_DOMAIN: &str = "VALUE";`
/// Returns `None` when the constant is not found.
fn extract_domain_value(content: &str) -> Option<String> {
    let marker = "SIGNATURE_DOMAIN";
    let mut search_from = 0;
    while let Some(pos) = content[search_from..].find(marker) {
        let start = search_from + pos;
        let line_start = content[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let line_end = content[start..].find('\n').map(|i| start + i).unwrap_or(content.len());
        let line = &content[line_start..line_end];

        // Find the opening quote after '='
        if let Some(eq_pos) = line.rfind('=') {
            let after_eq = line[eq_pos + 1..].trim();
            if let Some(val_start) = after_eq.find('"') {
                let inner = &after_eq[val_start + 1..];
                if let Some(val_end) = inner.find('"') {
                    return Some(inner[..val_end].to_string());
                }
            }
        }

        search_from = start + marker.len();
    }
    None
}

#[test]
fn signature_domains_are_unique_across_contracts() {
    let mut domains: HashMap<String, String> = HashMap::new();
    let mut missing: Vec<String> = Vec::new();
    let mut internal_duplicates: Vec<String> = Vec::new();

    for crate_name in CONTRACT_CRATES {
        let lib_path = format!("contracts/{crate_name}/src/lib.rs");
        let path = Path::new(&lib_path);

        let content = if path.exists() {
            fs::read_to_string(path).unwrap_or_else(|e| {
                panic!("failed to read {lib_path}: {e}")
            })
        } else {
            missing.push(format!("{lib_path} (file not found)"));
            continue;
        };

        let mut crate_values: Vec<String> = Vec::new();

        if let Some(val) = extract_domain_value(&content) {
            crate_values.push(val);
        }

        // Also check extra files for this crate (e.g. domain.rs)
        for &(extra_crate, extra_path) in EXTRA_DOMAIN_FILES {
            if extra_crate == *crate_name {
                if let Ok(extra_content) = fs::read_to_string(extra_path) {
                    if let Some(val) = extract_domain_value(&extra_content) {
                        crate_values.push(val);
                    }
                }
            }
        }

        if crate_values.is_empty() {
            missing.push(lib_path);
            continue;
        }

        // Deduplicate within the crate and check for cross-file inconsistency.
        let unique_in_crate: HashSet<&str> = crate_values.iter().map(|s| s.as_str()).collect();

        if unique_in_crate.len() > 1 {
            internal_duplicates.push(format!(
                "{crate_name}: multiple different values across files: {}",
                unique_in_crate.iter().map(|v| format!("\"{v}\"")).collect::<Vec<_>>().join(", ")
            ));
        }

        let first = crate_values.first().unwrap();
        domains.insert(crate_name.to_string(), first.clone());
    }

    assert!(
        missing.is_empty(),
        "The following contract crate(s) are missing a SIGNATURE_DOMAIN constant:\n  - {}\n\
         Each deployed contract must define a unique SIGNATURE_DOMAIN to prevent\n\
         cross-contract signature replay attacks. Add:\n\n\
             #[allow(dead_code)]\n\
             const SIGNATURE_DOMAIN: &str = \"...\";\n\n\
         near the top of the crate's lib.rs.",
        missing.join("\n  - ")
    );

    assert!(
        internal_duplicates.is_empty(),
        "Internal SIGNATURE_DOMAIN inconsistency:\n  - {}\n\
         All definitions within a single crate must agree on the same value.",
        internal_duplicates.join("\n  - ")
    );

    let mut seen: HashSet<&str> = HashSet::new();
    let mut duplicates: Vec<String> = Vec::new();

    for (crate_name, value) in &domains {
        if !seen.insert(value) {
            duplicates.push(format!("{crate_name}: \"{value}\""));
        }
    }

    assert!(
        duplicates.is_empty(),
        "Duplicate SIGNATURE_DOMAIN values detected (cross-contract replay risk):\n  - {}\n\
         Each contract must use a unique domain string.",
        duplicates.join("\n  - ")
    );

    let mut sorted: Vec<(&String, &String)> = domains.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    println!(
        "✓ All {} contract(s) define a unique SIGNATURE_DOMAIN:",
        sorted.len()
    );
    for (crate_name, value) in &sorted {
        println!("   {crate_name}: \"{value}\"");
    }
}