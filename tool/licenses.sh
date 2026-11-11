#!/bin/sh
# Write assets/licenses.txt: every crate that ends up in the Linux or macOS
# binary, with its version and licence, for the About page. Run it after a
# change of Cargo.lock; tests/packaging.rs fails on a line the lock file no
# longer has.
set -eu
cd "$(dirname "$0")/.."
cargo tree --locked -e normal --prefix none -f '{p}|{l}' \
    --target x86_64-unknown-linux-gnu \
    --target aarch64-apple-darwin \
    --target x86_64-apple-darwin |
    sed -e 's/ (\*)$//' -e 's/ (proc-macro)|/|/' |
    grep -v '^multigit ' |
    awk -F'|' '$1 != "" { split($1, p, " "); sub(/^v/, "", p[2]); print p[1] " " p[2] "\t" $2 }' |
    LC_ALL=C sort -u > assets/licenses.txt
