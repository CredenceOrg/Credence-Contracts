# default recipe – show available recipes
default:
    @just --list

# Run all workspace tests
test:
    cargo test --workspace

# Run tests for a single crate, optionally filtered by a test name.
#
# Usage:
#   just test-one credence_bond
#   just test-one credence_bond test_bond_create
#   just test-one credence_bond fuzz::test_bond_fuzz
test-one package test="":
    cargo test -p {{package}} {{test}}
