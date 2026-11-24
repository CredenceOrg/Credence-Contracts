import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from scripts import check_gated_packages as checker

SCRIPT = Path(checker.__file__).resolve()


class GatedPackageTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.write("Cargo.toml", """[workspace]
members = [
  "contracts/arbitration",
  "contracts/bounty-escrow",
  "contracts/math",
  "crates/testutils",
]
[workspace.package]
edition = "2021"
""")
        self.package("contracts/arbitration", "credence_arbitration", ["cdylib", "rlib"])
        self.package("contracts/bounty-escrow", "bounty-escrow", ["cdylib"])
        self.package("contracts/math", "math", ["rlib"])
        self.package("crates/testutils", "testutils")
        self.write("scripts/ci-gated-packages.txt", "# disabled package is ignored\ncredence_arbitration\nbounty-escrow\nmath\ntestutils\n")
        self.write("scripts/wasm-size-budget.toml", "default_kb = 64\n[contracts]\ncredence_arbitration = 64\nbounty_escrow = 32\n")

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def package(self, directory, name, crate_types=None, lib_name=None):
        text = f"[package]\nname = {json.dumps(name)}\nversion = \"0.1.0\"\n"
        if crate_types is not None or lib_name is not None:
            text += "[lib]\n"
            if crate_types is not None:
                text += f"crate-type = {json.dumps(crate_types)}\n"
            if lib_name is not None:
                text += f"name = {json.dumps(lib_name)}\n"
        self.write(f"{directory}/Cargo.toml", text)

    def check_both(self, expected=None, error=None):
        for backend in [checker.tomllib, None]:
            with self.subTest(parser="tomllib" if backend else "fallback"), patch.object(checker, "tomllib", backend):
                if error:
                    with self.assertRaisesRegex(checker.CheckError, error):
                        checker.check_repo(self.root)
                else:
                    self.assertEqual(checker.check_repo(self.root), expected)

    def test_valid_package_names_and_contract_budgets(self):
        self.check_both(("credence_arbitration", "bounty-escrow", "math", "testutils"))

    def test_workspace_inherited_package_fields_do_not_change_name(self):
        self.write("crates/testutils/Cargo.toml", "[package]\nname = 'testutils'\nversion.workspace = true\nedition.workspace = true\n")
        self.check_both(("credence_arbitration", "bounty-escrow", "math", "testutils"))

    def test_current_repository_matches_in_both_parsers(self):
        root = SCRIPT.parent.parent
        expected = checker.check_repo(root)
        with patch.object(checker, "tomllib", None):
            self.assertEqual(checker.check_repo(root), expected)

    def test_directory_name_is_not_package_name(self):
        self.write("scripts/ci-gated-packages.txt", "arbitration\n")
        self.check_both(error=r":1: unknown workspace package arbitration")

    def test_duplicate_entry_reports_both_line_numbers(self):
        self.write("scripts/ci-gated-packages.txt", "# comment\nmath\nmath\n")
        self.check_both(error=r":3: duplicate package math \(first at line 2\)")

    def test_trailing_space_and_tab_fail(self):
        for suffix in [" ", "\t"]:
            with self.subTest(suffix=repr(suffix)):
                self.write("scripts/ci-gated-packages.txt", "math" + suffix + "\n")
                self.check_both(error="trailing whitespace")

    def test_leading_whitespace_fails_like_workflow(self):
        self.write("scripts/ci-gated-packages.txt", "  math\n")
        self.check_both(error="leading whitespace")

    def test_inline_comments_and_multiple_names_fail(self):
        for line in ["math # comment", "math testutils", "--help"]:
            with self.subTest(line=line):
                self.write("scripts/ci-gated-packages.txt", line + "\n")
                self.check_both(error="expected one Cargo package name|unknown workspace package")

    def test_blank_lines_crlf_and_comments_are_ignored(self):
        self.write("scripts/ci-gated-packages.txt", "\r\n \t\r\n  # missing-package \t\r\nmath\r\n")
        self.check_both(("math",))

    def test_last_line_without_newline_is_checked(self):
        self.write("scripts/ci-gated-packages.txt", "missing")
        self.check_both(error="unknown workspace package missing")

    def test_empty_gate_is_valid(self):
        self.write("scripts/ci-gated-packages.txt", "# nothing enabled\n")
        self.check_both(())

    def test_only_gated_contracts_need_budgets(self):
        self.write("scripts/ci-gated-packages.txt", "math\ntestutils\n")
        (self.root / "scripts/wasm-size-budget.toml").unlink()
        self.check_both(("math", "testutils"))

    def test_default_budget_does_not_replace_explicit_contract_entry(self):
        self.write("scripts/wasm-size-budget.toml", "default_kb = 64\n[contracts]\ncredence_arbitration = 64\n")
        self.check_both(error="contract bounty-escrow needs.*bounty_escrow")

    def test_budget_must_be_positive_integer(self):
        for budget in ["0", '"64"']:
            with self.subTest(budget=budget):
                self.write("scripts/wasm-size-budget.toml", f"[contracts]\ncredence_arbitration = {budget}\nbounty_escrow = 32\n")
                self.check_both(error="positive integer KiB budget")

    def test_bool_and_float_are_not_integer_budgets(self):
        for budget in ["true", "64.5"]:
            self.write("scripts/wasm-size-budget.toml", f"[contracts]\ncredence_arbitration = {budget}\nbounty_escrow = 32\n")
            self.check_both(error="positive integer KiB budget|unsupported TOML value")

    def test_explicit_library_name_matches_wasm_budget(self):
        self.package("contracts/bounty-escrow", "bounty-escrow", ["cdylib"], "custom_contract")
        self.write("scripts/wasm-size-budget.toml", "[contracts]\ncredence_arbitration = 64\ncustom_contract = 32\n")
        self.check_both(("credence_arbitration", "bounty-escrow", "math", "testutils"))

    def test_cdylib_outside_contracts_directory_needs_budget(self):
        self.package("crates/testutils", "testutils", ["cdylib"])
        self.check_both(error="contract testutils needs.*testutils")

    def test_glob_members_exclusions_and_literal_toml_strings(self):
        self.write("Cargo.toml", """[workspace]
members = ['contracts/*', 'crates/*'] # all declared directories
exclude = ['contracts/bounty-escrow']
""")
        self.write("scripts/ci-gated-packages.txt", "credence_arbitration\nmath\ntestutils\n")
        self.check_both(("credence_arbitration", "math", "testutils"))
        self.write("scripts/ci-gated-packages.txt", "bounty-escrow\n")
        self.check_both(error="unknown workspace package bounty-escrow")

    def test_unlisted_directory_does_not_become_workspace_package(self):
        self.package("contracts/not-a-member", "not_a_member", ["cdylib"])
        self.write("scripts/ci-gated-packages.txt", "not_a_member\n")
        self.check_both(error="unknown workspace package not_a_member")

    def test_root_package_in_nonvirtual_workspace(self):
        self.write("Cargo.toml", "[package]\nname = 'root_helper'\n[workspace]\nmembers = []\n")
        self.write("scripts/ci-gated-packages.txt", "root_helper\n")
        self.check_both(("root_helper",))

    def test_missing_member_or_manifest_fails(self):
        self.write("Cargo.toml", "[workspace]\nmembers = ['contracts/missing']\n")
        self.check_both(error="workspace member has no match")
        (self.root / "contracts/missing").mkdir()
        self.check_both(error="Cargo.toml")

    def test_duplicate_workspace_package_names_fail(self):
        self.package("contracts/math", "testutils", ["rlib"])
        self.check_both(error="duplicate workspace package testutils")

    def test_empty_or_invalid_workspace_fails(self):
        for text in ["[workspace]\nmembers = []\n", "[package]\nname = 'standalone'\n"]:
            self.write("Cargo.toml", text)
            self.check_both(error="no packages|must contain ")

    def test_malformed_relevant_manifest_values_fail(self):
        for text in ["[workspace]\nmembers = 'contracts/*'\n", "[workspace]\nmembers = [7]\n"]:
            self.write("Cargo.toml", text)
            self.check_both(error="array of nonempty strings|unsupported TOML value")

    def test_invalid_member_package_name_fails(self):
        for name in ["", "bad name"]:
            self.package("contracts/math", name)
            self.check_both(error="missing or invalid ")

    def test_invalid_library_and_budget_tables_fail(self):
        self.write("contracts/math/Cargo.toml", "[package]\nname = 'math'\nlib = 'ignored'\n[lib]\ncrate-type = ['rlib']\nname = 'math'\n")
        self.check_both(("credence_arbitration", "bounty-escrow", "math", "testutils"))
        self.package("contracts/bounty-escrow", "bounty-escrow", ["cdylib"], "bad-library-name")
        self.check_both(error="invalid WASM library name")
        self.package("contracts/bounty-escrow", "bounty-escrow", ["cdylib"])
        self.write("scripts/wasm-size-budget.toml", "contracts = 64\n")
        self.check_both(error="table|needs a positive integer")

    def test_fallback_rejects_missing_array_separator_and_assignment(self):
        for text in ["[workspace]\nmembers = ['a' 'b']\n", "[workspace]\nmembers is invalid\n"]:
            self.write("Cargo.toml", text)
            with patch.object(checker, "tomllib", None):
                with self.assertRaisesRegex(checker.CheckError, "Python 3.11"):
                    checker.check_repo(self.root)

    def test_path_escape_is_rejected(self):
        self.write("Cargo.toml", "[workspace]\nmembers = ['../outside']\n")
        self.check_both(error="must stay within the repository")

    def test_symlink_escape_is_rejected(self):
        with tempfile.TemporaryDirectory() as outside:
            (self.root / "outside").symlink_to(outside, target_is_directory=True)
            self.write("Cargo.toml", "[workspace]\nmembers = ['outside']\n")
            self.check_both(error="escapes the repository")

    def test_missing_or_unreadable_gate_and_budget_fail(self):
        gate = self.root / "scripts/ci-gated-packages.txt"
        gate.unlink()
        self.check_both(error="ci-gated-packages.txt")
        gate.write_bytes(b"\xff")
        self.check_both(error="ci-gated-packages.txt")
        gate.write_text("credence_arbitration\n", encoding="utf-8")
        (self.root / "scripts/wasm-size-budget.toml").unlink()
        self.check_both(error="wasm-size-budget.toml")

    def test_duplicate_budget_assignment_is_rejected(self):
        self.write("scripts/wasm-size-budget.toml", "[contracts]\ncredence_arbitration = 64\ncredence_arbitration = 32\n")
        self.check_both(error="overwrite|duplicate")

    def test_comments_inside_quoted_strings_are_not_stripped(self):
        self.package("contracts/math", "math#not-comment")
        self.check_both(error="missing or invalid ")

    def test_fallback_rejects_unsupported_relevant_string_escapes(self):
        self.write("Cargo.toml", '[workspace]\nmembers = ["contracts\\u002fmath"]\n')
        with patch.object(checker, "tomllib", None):
            with self.assertRaisesRegex(checker.CheckError, "Python 3.11"):
                checker.check_repo(self.root)

    def test_main_success_and_error_messages(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(checker.main(["--root", str(self.root)]), 0)
        self.assertIn("OK: 4 gated packages", out.getvalue())
        self.write("scripts/ci-gated-packages.txt", "unknown\n")
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            self.assertEqual(checker.main(["--root", str(self.root)]), 1)
        self.assertIn("ERROR:", err.getvalue())
        self.assertIn(":1: unknown workspace package unknown", err.getvalue())

    def test_cli_is_read_only_and_needs_no_cargo_or_network(self):
        before = {p.relative_to(self.root): p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
        result = subprocess.run([sys.executable, str(SCRIPT), "--root", str(self.root)],
                                cwd=self.directory.name, text=True, capture_output=True,
                                env={**os.environ, "PATH": "", "PYTHONDONTWRITEBYTECODE": "1"})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("OK: 4 gated packages", result.stdout)
        after = {p.relative_to(self.root): p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
        self.assertEqual(before, after)
        self.write("scripts/ci-gated-packages.txt", "math\nmath\n")
        result = subprocess.run([sys.executable, str(SCRIPT), "--root", str(self.root)],
                                text=True, capture_output=True, env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"})
        self.assertEqual(result.returncode, 1)
        self.assertIn("duplicate package math", result.stderr)


if __name__ == "__main__":
    unittest.main()
