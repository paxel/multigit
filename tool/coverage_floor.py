"""A line coverage floor that may only rise.

    python3 tool/coverage_floor.py <lcov file> [floor file [repository root]]

Keeps only the records of the crate's own sources (src/ below the
repository root) in the LCOV file, rewritten in place so SonarQube Cloud
imports what the floor measured, computes the line coverage, and compares
it with the floor in the floor file (default: tool/coverage-floor.txt):

- below the floor: exit 1;
- the measured value rounded down, less one point, above the floor: the
  floor is raised to that in the file, for the user to commit;
- otherwise the file is left alone. A floor is never lowered.
"""

import math
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
FLOOR = pathlib.Path(__file__).with_name("coverage-floor.txt")


def records(text):
    """The LCOV text split into records, each a list of lines ending at
    end_of_record."""
    out, current = [], []
    for line in text.splitlines():
        if not line.strip():
            continue
        current.append(line)
        if line == "end_of_record":
            out.append(current)
            current = []
    if current:
        out.append(current)
    return out


def source(record):
    for line in record:
        if line.startswith("SF:"):
            return line[3:]
    return ""


def own(path, root):
    """True for a file below <root>/src; relative paths count from root."""
    path = pathlib.PurePath(path.replace("\\", "/"))
    if path.is_absolute():
        try:
            path = path.relative_to(root)
        except ValueError:
            return False
    return path.parts[:1] == ("src",)


def keep(text, root):
    return [r for r in records(text) if own(source(r), root)]


def line_coverage(kept):
    """Percent of instrumented lines hit, from the DA: lines."""
    found = hit = 0
    for record in kept:
        for line in record:
            if line.startswith("DA:"):
                found += 1
                if int(line[3:].split(",")[1]) > 0:
                    hit += 1
    return 100.0 * hit / found if found else 0.0


def read_floor(path):
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if line:
            return int(line)
    raise ValueError(f"no floor in {path}")


def write_floor(path, value):
    lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
    for i, line in enumerate(lines):
        if line.split("#", 1)[0].strip():
            lines[i] = f"{value}\n"
            break
    path.write_text("".join(lines), encoding="utf-8")


def main(argv):
    if not 1 <= len(argv) <= 3:
        print(__doc__, file=sys.stderr)
        return 2
    lcov = pathlib.Path(argv[0])
    floor_path = pathlib.Path(argv[1]) if len(argv) >= 2 else FLOOR
    root = pathlib.PurePath(argv[2]) if len(argv) == 3 else ROOT
    kept = keep(lcov.read_text(encoding="utf-8"), root)
    if not kept:
        print(f"no source files in {lcov}", file=sys.stderr)
        return 1
    lcov.write_text("".join("\n".join(r) + "\n" for r in kept), encoding="utf-8")
    measured = line_coverage(kept)
    floor = read_floor(floor_path)
    # One point below the measured value, rounded down: the margin that
    # keeps a CI run measuring a fraction lower from stopping a release.
    target = math.floor(measured) - 1
    print(f"{measured:.2f}% of lines covered, floor {floor}%")
    if measured < floor:
        print(f"below the floor of {floor}%", file=sys.stderr)
        return 1
    if target > floor:
        write_floor(floor_path, target)
        print(f"floor raised to {target}% in {floor_path}; commit it")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
