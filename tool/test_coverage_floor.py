import pathlib
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from io import StringIO

from coverage_floor import main


def record(path, hits):
    lines = [f"SF:{path}"]
    lines += [f"DA:{n},{h}" for n, h in enumerate(hits, start=1)]
    lines += [f"LF:{len(hits)}", f"LH:{sum(1 for h in hits if h)}", "end_of_record"]
    return "\n".join(lines) + "\n"


FLOOR = "# comment\n{floor}\n"
ROOT = "/work/multigit"


class CoverageFloorTest(unittest.TestCase):
    def run_floor(self, floor, lcov_text):
        with tempfile.TemporaryDirectory() as tmp:
            floor_file = pathlib.Path(tmp) / "floor.txt"
            floor_file.write_text(FLOOR.format(floor=floor), encoding="utf-8")
            lcov = pathlib.Path(tmp) / "lcov.info"
            lcov.write_text(lcov_text, encoding="utf-8")
            with redirect_stdout(StringIO()), redirect_stderr(StringIO()):
                code = main([str(lcov), str(floor_file), ROOT])
            return (
                code,
                floor_file.read_text(encoding="utf-8"),
                lcov.read_text(encoding="utf-8"),
            )

    # 3 of 4 lines: 75 %.
    MAIN = record(f"{ROOT}/src/main.rs", [1, 0, 2, 3])

    def test_below_the_floor_fails_and_keeps_the_floor(self):
        code, floor, _ = self.run_floor(76, self.MAIN)
        self.assertEqual(code, 1)
        self.assertEqual(floor, FLOOR.format(floor=76))

    def test_above_the_floor_raises_it_to_one_point_below_rounded_down(self):
        # 2 of 3 lines: 66.67 % is raised to 65.
        code, floor, _ = self.run_floor(60, record("src/a.rs", [1, 1, 0]))
        self.assertEqual(code, 0)
        self.assertEqual(floor, FLOOR.format(floor=65))

    def test_equal_to_the_floor_leaves_the_file_alone(self):
        code, floor, _ = self.run_floor(75, self.MAIN)
        self.assertEqual(code, 0)
        self.assertEqual(floor, FLOOR.format(floor=75))

    def test_one_point_above_the_floor_leaves_it_alone(self):
        # 76.5 %: one point below it, rounded down, is the floor itself.
        hits = [1] * 153 + [0] * 47
        code, floor, _ = self.run_floor(75, record("src/a.rs", hits))
        self.assertEqual(code, 0)
        self.assertEqual(floor, FLOOR.format(floor=75))

    def test_a_zero_floor_is_raised_by_the_first_run(self):
        code, floor, _ = self.run_floor(0, self.MAIN)
        self.assertEqual(code, 0)
        self.assertEqual(floor, FLOOR.format(floor=74))

    def test_tests_dependencies_and_other_roots_are_dropped(self):
        text = (
            self.MAIN
            + record(f"{ROOT}/tests/packaging.rs", [0] * 100)
            + record("/home/runner/.cargo/registry/src/x/egui/src/lib.rs", [0] * 100)
            + record("/elsewhere/src/lib.rs", [0] * 100)
        )
        code, floor, lcov = self.run_floor(75, text)
        self.assertEqual(code, 0)
        self.assertEqual(floor, FLOOR.format(floor=75))
        self.assertEqual(lcov, self.MAIN)

    def test_no_source_file_fails(self):
        code, _, _ = self.run_floor(0, record(f"{ROOT}/tests/a.rs", [1]))
        self.assertEqual(code, 1)


if __name__ == "__main__":
    unittest.main()
