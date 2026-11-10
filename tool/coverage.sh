#!/usr/bin/env bash
# Coverage floor: runs the tests with coverage, writes coverage/lcov.info
# for SonarQube Cloud (sonar-project.properties), and checks the line
# coverage of src/ against tool/coverage-floor.txt with
# tool/coverage_floor.py.
#
# Needs cargo-llvm-cov and the llvm-tools-preview rustup component.
# COVERAGE_JOBS=N limits the build and test parallelism (for a shared
# machine); unset leaves cargo's default.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

jobs=${COVERAGE_JOBS:-}
mkdir -p coverage
if [ -n "$jobs" ]; then
  export RUST_TEST_THREADS=$jobs
fi
cargo llvm-cov --locked ${jobs:+-j "$jobs"} --lcov --output-path coverage/lcov.info
python3 tool/coverage_floor.py coverage/lcov.info
