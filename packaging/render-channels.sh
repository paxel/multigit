#!/bin/sh
# Render the Homebrew formula (Linux) and cask (macOS) from the templates in
# packaging/homebrew, with the checksums of the release artifacts.
#
#   packaging/render-channels.sh <version> <dist dir> <out dir>
#
# Writes <out dir>/multigit-formula.rb and <out dir>/multigit-cask.rb.
set -eu

version=$1
dist=$2
out=$3

case "$version" in
    "" | *[!0-9A-Za-z.+-]*) echo "invalid version: $version" >&2; exit 1 ;;
esac

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
sha() {
    file="$dist/$1"
    [ -f "$file" ] || { echo "missing artifact: $file" >&2; exit 1; }
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$file" | cut -d' ' -f1
    else
        shasum -a 256 "$file" | cut -d' ' -f1
    fi
}

linux=$(sha "multigit-$version-x86_64-linux.tar.gz")
macos=$(sha "multigit-$version-macos-universal.zip")
mkdir -p "$out"
sed -e "s/{{VERSION}}/$version/g" -e "s/{{SHA_LINUX_X86_64}}/$linux/" \
    "$here/homebrew/formula.rb.tmpl" > "$out/multigit-formula.rb"
sed -e "s/{{VERSION}}/$version/g" -e "s/{{SHA_MACOS_UNIVERSAL}}/$macos/" \
    "$here/homebrew/cask.rb.tmpl" > "$out/multigit-cask.rb"
ls -1 "$out"
