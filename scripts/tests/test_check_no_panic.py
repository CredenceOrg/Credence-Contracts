import contextlib
import importlib.util
import io
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SCRIPT_PATH = Path(__file__).resolve().parent.parent / "check_no_panic.py"


def _load_check_no_panic():
    spec = importlib.util.spec_from_file_location("check_no_panic", SCRIPT_PATH)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


check_no_panic = _load_check_no_panic()


class CheckNoPanicTests(unittest.TestCase):
    def _make_workspace(self, tmpdir: str, files: dict[str, str], baseline_lines: list[str] | None = None) -> Path:
        root = Path(tmpdir)
        for rel, content in files.items():
            path = root / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
        baseline_path = root / "scripts" / "panic_baseline.txt"
        baseline_path.parent.mkdir(parents=True, exist_ok=True)
        if baseline_lines is not None:
            baseline_path.write_text("\n".join(baseline_lines) + "\n", encoding="utf-8")
        return root

    def _run_main(self, root: Path) -> tuple[int, str, str]:
        stdout = io.StringIO()
        stderr = io.StringIO()
        with (
            patch.object(check_no_panic, "ROOT", root),
            patch.object(check_no_panic, "CONTRACTS", root / "contracts"),
            patch.object(check_no_panic, "BASELINE_PATH", root / "scripts" / "panic_baseline.txt"),
            patch.object(sys, "argv", ["check_no_panic.py"]),
            contextlib.redirect_stdout(stdout),
            contextlib.redirect_stderr(stderr),
        ):
            code = check_no_panic.main()
        return code, stdout.getvalue(), stderr.getvalue()

    def _run_scan(self, root: Path) -> list[tuple[str, int, str]]:
        with (
            patch.object(check_no_panic, "ROOT", root),
            patch.object(check_no_panic, "CONTRACTS", root / "contracts"),
        ):
            return check_no_panic.scan()

    def test_is_test_gate_line(self):
        self.assertTrue(check_no_panic.is_test_gate_line("#[cfg(test)]"))
        self.assertTrue(check_no_panic.is_test_gate_line('    #[cfg(any(test, feature = "bench"))]'))
        self.assertTrue(check_no_panic.is_test_gate_line('#[cfg(feature = "testutils")]'))
        self.assertFalse(check_no_panic.is_test_gate_line("#[cfg(not(test))]"))
        self.assertFalse(check_no_panic.is_test_gate_line('#[cfg(target_arch = "wasm32")]'))
        self.assertFalse(check_no_panic.is_test_gate_line("pub fn helper() {}"))

    def test_test_gated_module_excluded(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            root = self._make_workspace(
                tmpdir,
                {
                    "contracts/demo/src/lib.rs": (
                        "pub fn ok() {}\n"
                        "\n"
                        "#[cfg(test)]\n"
                        "mod tests;\n"
                        "\n"
                        "#[cfg(test)]\n"
                        "mod inline_tests {\n"
                        "    fn t() {\n"
                        '        panic!("inline test panic");\n'
                        "    }\n"
                        "}\n"
                    ),
                    "contracts/demo/src/tests.rs": (
                        "fn test_case() {\n"
                        '    panic!("external test panic");\n'
                        "}\n"
                    ),
                },
                baseline_lines=[],
            )

            self.assertEqual(self._run_scan(root), [])
            code, stdout, stderr = self._run_main(root)
            self.assertEqual(code, 0)
            self.assertIn("0 baselined, 0 new", stdout)
            self.assertEqual(stderr, "")

    def test_cfg_not_test_included(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            root = self._make_workspace(
                tmpdir,
                {
                    "contracts/demo/src/lib.rs": (
                        "#[cfg(not(test))]\n"
                        "pub fn release_only() {\n"
                        '    panic!("release panic");\n'
                        "}\n"
                    ),
                },
                baseline_lines=[],
            )

            results = self._run_scan(root)
            expected_rel = str(Path("contracts/demo/src/lib.rs"))
            self.assertEqual(results, [(expected_rel, 3, 'panic!("release panic")')])

            code, _stdout, stderr = self._run_main(root)
            self.assertEqual(code, 1)
            self.assertIn(f'{expected_rel}:3: panic!("release panic")', stderr)

    def test_path_attribute_module_resolution(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            root = self._make_workspace(
                tmpdir,
                {
                    "contracts/demo/src/lib.rs": (
                        "pub fn ok() {}\n"
                        "\n"
                        "#[cfg(test)]\n"
                        '#[path = "custom_tests.rs"]\n'
                        "mod tests;\n"
                    ),
                    "contracts/demo/src/custom_tests.rs": (
                        "fn test_via_path() {\n"
                        '    panic!("path-resolved test panic");\n'
                        "}\n"
                    ),
                },
                baseline_lines=[],
            )

            lib_rs = root / "contracts" / "demo" / "src" / "lib.rs"
            custom_tests_rs = root / "contracts" / "demo" / "src" / "custom_tests.rs"
            reachable, test_gated = check_no_panic.parse_mod_graph(lib_rs)

            self.assertIn(custom_tests_rs, reachable)
            self.assertIn(custom_tests_rs, test_gated)
            self.assertEqual(self._run_scan(root), [])

            code, _stdout, stderr = self._run_main(root)
            self.assertEqual(code, 0)
            self.assertEqual(stderr, "")

    def test_baselined_entry_tolerated(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            expected_rel = str(Path("contracts/demo/src/lib.rs"))
            key = check_no_panic.baseline_key(expected_rel, 'panic!("legacy debt")')
            root = self._make_workspace(
                tmpdir,
                {
                    "contracts/demo/src/lib.rs": (
                        "pub fn legacy() {\n"
                        '    panic!("legacy debt");\n'
                        "}\n"
                    ),
                },
                baseline_lines=["# comment line", key],
            )

            results = self._run_scan(root)
            self.assertEqual(results, [(expected_rel, 2, 'panic!("legacy debt")')])

            code, stdout, stderr = self._run_main(root)
            self.assertEqual(code, 0)
            self.assertIn("1 baselined, 0 new", stdout)
            self.assertEqual(stderr, "")

    def test_new_entry_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            root = self._make_workspace(
                tmpdir,
                {
                    "contracts/demo/src/lib.rs": (
                        "pub fn ok() {}\n"
                        "\n"
                        "pub fn bad() {\n"
                        '    panic!("unexpected failure");\n'
                        "}\n"
                    ),
                },
                baseline_lines=[],
            )

            expected_rel = str(Path("contracts/demo/src/lib.rs"))
            code, stdout, stderr = self._run_main(root)
            self.assertEqual(code, 1)
            self.assertEqual(stdout, "")
            self.assertIn(f'{expected_rel}:4: panic!("unexpected failure")', stderr)

    def test_panic_with_error_and_comments_ignored(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            root = self._make_workspace(
                tmpdir,
                {
                    "contracts/demo/src/lib.rs": (
                        "pub fn safe() {\n"
                        '    // panic!("commented out")\n'
                        "    panic_with_error!(env, ContractError::Invalid);\n"
                        "}\n"
                    ),
                },
                baseline_lines=[],
            )

            self.assertEqual(self._run_scan(root), [])
            code, _stdout, stderr = self._run_main(root)
            self.assertEqual(code, 0)
            self.assertEqual(stderr, "")


if __name__ == "__main__":
    unittest.main()
