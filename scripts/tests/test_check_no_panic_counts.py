"""Tests for the per-key occurrence counts in scripts/check_no_panic.py.

The baseline ratchet keys on `path::panic!(...)` snippets; identical
messages in the same file share a key, so the baseline stores an
occurrence count per key and fails when the count grows.
"""
import importlib.util
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().parents[1] / "check_no_panic.py"
spec = importlib.util.spec_from_file_location("check_no_panic", MODULE_PATH)
check_no_panic = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check_no_panic)


def found(*entries):
    """Build a `found` list of (relpath, line, snippet) tuples."""
    return list(entries)


class FindNewViolationsTest(unittest.TestCase):
    def test_equal_counts_pass(self):
        f = found(
            ("a.rs", 1, 'panic!("x")'),
            ("a.rs", 2, 'panic!("x")'),
            ("b.rs", 5, 'panic!("y")'),
        )
        baseline = {
            "a.rs::panic!(\"x\")": 2,
            "b.rs::panic!(\"y\")": 1,
        }
        self.assertEqual(check_no_panic.find_new_violations(f, baseline), [])

    def test_lower_counts_pass(self):
        f = found(("a.rs", 1, 'panic!("x")'))
        baseline = {"a.rs::panic!(\"x\")": 3}
        self.assertEqual(check_no_panic.find_new_violations(f, baseline), [])

    def test_higher_count_fails_on_excess_only(self):
        f = found(
            ("a.rs", 1, 'panic!("x")'),
            ("a.rs", 2, 'panic!("x")'),
            ("a.rs", 3, 'panic!("x")'),
        )
        baseline = {"a.rs::panic!(\"x\")": 1}
        violations = check_no_panic.find_new_violations(f, baseline)
        self.assertEqual(
            violations,
            [
                ("a.rs", 2, 'panic!("x")', 2),
                ("a.rs", 3, 'panic!("x")', 3),
            ],
        )

    def test_new_key_fails(self):
        f = found(("a.rs", 1, 'panic!("new")'))
        violations = check_no_panic.find_new_violations(f, {})
        self.assertEqual(violations, [("a.rs", 1, 'panic!("new")', 1)])

    def test_counts_are_per_key(self):
        # Same snippet in different files are different keys.
        f = found(
            ("a.rs", 1, 'panic!("x")'),
            ("b.rs", 1, 'panic!("x")'),
        )
        baseline = {
            "a.rs::panic!(\"x\")": 1,
            "b.rs::panic!(\"x\")": 1,
        }
        self.assertEqual(check_no_panic.find_new_violations(f, baseline), [])


class BaselineCountsTest(unittest.TestCase):
    def test_counts_occurrences(self):
        f = found(
            ("a.rs", 1, 'panic!("x")'),
            ("a.rs", 2, 'panic!("x")'),
            ("b.rs", 1, 'panic!("y")'),
        )
        self.assertEqual(
            check_no_panic.baseline_counts(f),
            {"a.rs::panic!(\"x\")": 2, "b.rs::panic!(\"y\")": 1},
        )


class LoadWriteBaselineTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self._orig = check_no_panic.BASELINE_PATH
        check_no_panic.BASELINE_PATH = Path(self._tmp.name) / "baseline.txt"
        self.addCleanup(setattr, check_no_panic, "BASELINE_PATH", self._orig)

    def test_roundtrip(self):
        counts = {"a.rs::panic!(\"x\")": 2, "b.rs::panic!(\"y\")": 1}
        check_no_panic.write_baseline(counts)
        self.assertEqual(check_no_panic.load_baseline(), counts)

    def test_missing_file_returns_empty(self):
        self.assertEqual(check_no_panic.load_baseline(), {})

    def test_comments_and_blanks_skipped(self):
        check_no_panic.BASELINE_PATH.write_text(
            "# comment\n\na.rs::panic!(\"x\")\t2\n", encoding="utf-8"
        )
        self.assertEqual(
            check_no_panic.load_baseline(), {"a.rs::panic!(\"x\")": 2}
        )

    def test_malformed_line_raises(self):
        check_no_panic.BASELINE_PATH.write_text(
            "a.rs::panic!(\"x\")\n", encoding="utf-8"
        )
        with self.assertRaises(ValueError):
            check_no_panic.load_baseline()


if __name__ == "__main__":
    unittest.main()
