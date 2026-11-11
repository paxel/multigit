#!/bin/sh
# Build multigit.app from a compiled binary. Needs macOS (sips, iconutil).
# Usage: bundle.sh <binary> <version> <output dir>
set -eu

binary="$1"
version="$2"
out="$3"

case "$version" in
    "" | *[!0-9A-Za-z.+-]*) echo "invalid version: $version" >&2; exit 1 ;;
esac

root=$(cd "$(dirname "$0")/../.." && pwd)
app="$out/multigit.app"

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/multigit"
chmod 755 "$app/Contents/MacOS/multigit"
sed "s/@VERSION@/$version/g" "$root/packaging/macos/Info.plist" > "$app/Contents/Info.plist"

# The icon set from the 256 px source; larger sizes would only be upscaled.
iconset=$(mktemp -d)/multigit.iconset
mkdir -p "$iconset"
for size in 16 32 128 256; do
    sips -z "$size" "$size" "$root/assets/icon_256.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    half=$((size / 2))
    if [ "$half" -ge 16 ]; then
        cp "$iconset/icon_${size}x${size}.png" "$iconset/icon_${half}x${half}@2x.png"
    fi
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/multigit.icns"
rm -rf "$(dirname "$iconset")"

# The licences that have to travel with the binary: multigit's own and the
# one of the font built into it.
cp "$root/LICENSE" "$root/assets/fonts/Fredoka-OFL.txt" "$app/Contents/Resources/"

# Ad-hoc signature over the whole bundle: Apple silicon runs no unsigned code,
# and a quarantined download (the Homebrew cask) with only the linker's
# signature on the binary is reported as damaged instead of offering Open Anyway.
codesign --force --sign - "$app"

echo "$app"
