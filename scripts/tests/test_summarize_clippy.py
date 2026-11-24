import io
import tempfile
import unittest
from collections import Counter
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

from scripts.summarize_clippy import (
    SummaryError,
    baseline_regressions,
    format_summary,
    main,
    parse_json_lines,
    read_counts,
)

FIXTURES = Path(__file__).resolve().parent.parent / "testdata" / "clippy"
SAMPLE = FIXTURES / "sample.json"
BASELINE = FIXTURES / "baseline.json"
CRATE_A = "path+file:///workspace#crate-a@0.1.0"
CRATE_Y = "path+file:///workspace#crate-y@0.1.0"
CRATE_Z = "path+file:///workspace#crate-z@0.1.0"


class SummarizeClippyTests(unittest.TestCase):
    def test_counts_clippy_messages_by_package_lint_and_level(self):
        counts = read_counts(SAMPLE)

        self.assertEqual(counts[(CRATE_A, "clippy::expect_used", "error")], 1)
        self.assertEqual(counts[(CRATE_A, "clippy::unwrap_used", "warning")], 1)
        self.assertEqual(counts[(CRATE_Z, "clippy::unwrap_used", "warning")], 2)
        self.assertEqual(counts[(CRATE_Z, "clippy::unwrap_used", "error")], 1)
        self.assertEqual(sum(counts.values()), 6)
        self.assertNotIn((CRATE_A, "rustc::unused_imports", "warning"), counts)

    def test_baseline_shows_sorted_rows_and_burn_down(self):
        output = format_summary(read_counts(SAMPLE), read_counts(BASELINE))
        rows = [
            line
            for line in output.splitlines()
            if line.startswith(("path+", "TOTAL"))
        ]

        self.assertEqual(
            [row.split(" | ")[0].strip() for row in rows],
            [CRATE_A, CRATE_A, CRATE_Y, CRATE_Z, CRATE_Z, "TOTAL"],
        )
        self.assertIn("Baseline findings: 8 (delta: -2)", output)
        crate_y_row = next(row for row in rows if row.startswith(CRATE_Y))
        self.assertEqual(
            [field.strip() for field in crate_y_row.split("|")],
            [CRATE_Y, "clippy::todo", "0", "0", "0", "1", "-1"],
        )
        self.assertEqual(output, format_summary(read_counts(SAMPLE), read_counts(BASELINE)))

    def test_baseline_regressions_are_sorted_and_pair_specific(self):
        current = read_counts(SAMPLE)
        baseline = read_counts(BASELINE)

        self.assertEqual(baseline_regressions(current, baseline), [])
        regressions = baseline_regressions(current, Counter())
        self.assertEqual(
            [pair for pair, _, _ in regressions],
            [
                (CRATE_A, "clippy::expect_used"),
                (CRATE_A, "clippy::unwrap_used"),
                (CRATE_Z, "clippy::arithmetic_side_effects"),
                (CRATE_Z, "clippy::unwrap_used"),
            ],
        )

    def test_fail_on_threshold_is_inclusive(self):
        stdout = io.StringIO()
        stderr = io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            exit_code = main([str(SAMPLE), "--fail-on", "6"])
        self.assertEqual(exit_code, 0)
        self.assertEqual(stderr.getvalue(), "")

        stdout = io.StringIO()
        stderr = io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            exit_code = main([str(SAMPLE), "--fail-on", "5"])
        self.assertEqual(exit_code, 1)
        self.assertIn("6 findings exceed --fail-on threshold 5", stderr.getvalue())

    def test_baseline_regression_fails_cli(self):
        stdout = io.StringIO()
        stderr = io.StringIO()
        with tempfile.TemporaryDirectory() as temp_dir:
            empty_baseline = Path(temp_dir) / "empty.json"
            empty_baseline.write_text(
                '{"reason":"build-finished","success":true}\n',
                encoding="utf-8",
            )
            with redirect_stdout(stdout), redirect_stderr(stderr):
                exit_code = main([str(SAMPLE), "--baseline", str(empty_baseline)])

        self.assertEqual(exit_code, 1)
        self.assertIn("4 package/lint count(s) exceed the baseline", stderr.getvalue())

    def test_malformed_json_reports_source_line(self):
        with self.assertRaisesRegex(SummaryError, r"fixture\.json:2: invalid JSON"):
            parse_json_lines(io.StringIO("\nnot-json\n"), "fixture.json")


if __name__ == "__main__":
    unittest.main()
