#!/bin/sh
# Install the multigit menu entry and its icon into the current user's XDG
# data directory, so the app appears in the application menu. Used by the
# Homebrew formula, which installs the binary itself. Idempotent;
# --uninstall takes them out again.
#
# Homebrew's own share directory is not on a desktop session's XDG_DATA_DIRS,
# so an entry installed there alone stays invisible to menus. On Wayland the
# entry also gives the window its icon: the compositor looks for the entry
# named after the window's app id. The app writes the same entry on every
# start.
#
# Usage:
#   install-icon.sh                menu entry and icon
#   install-icon.sh --uninstall    remove them again
set -eu

mode=install
for argument in "$@"; do
    case $argument in
        -h | --help)
            sed -n '2,16p' "$0"
            exit 0
            ;;
        --uninstall)
            mode=uninstall
            ;;
        *)
            echo "install-icon: unknown argument $argument" >&2
            exit 2
            ;;
    esac
done

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
app_id=io.github.paxel.multigit
data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
desktop_target="$data_home/applications/$app_id.desktop"
png_target="$data_home/icons/hicolor/256x256/apps/multigit.png"

refresh_caches() {
    # Where the tools exist; menus also pick the files up on their own.
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -f -t "$data_home/icons/hicolor" >/dev/null 2>&1 || true
    fi
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$data_home/applications" >/dev/null 2>&1 || true
    fi
    if command -v kbuildsycoca6 >/dev/null 2>&1; then
        kbuildsycoca6 >/dev/null 2>&1 || true
    fi
}

if [ "$mode" = uninstall ]; then
    for target in "$desktop_target" "$png_target"; do
        if [ -e "$target" ]; then
            rm -f "$target"
            echo "  removed $target"
        fi
    done
    refresh_caches
    echo "multigit is out of your application menu."
    exit 0
fi

mkdir -p "$(dirname "$desktop_target")" "$(dirname "$png_target")"
cp "$here/icon_256.png" "$png_target"
echo "  $png_target"
cp "$here/$app_id.desktop" "$desktop_target"
echo "  $desktop_target"
refresh_caches
echo "multigit is in your application menu. A window already open keeps its old"
echo "icon; start multigit again to see the new one."
