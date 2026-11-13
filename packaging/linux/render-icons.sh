#!/bin/sh
# Regenerate the hicolor icon set from assets/zerotier-gui.svg.
# Requires rsvg-convert (librsvg2-bin).
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
source="$root/assets/zerotier-gui.svg"

for size in 16 24 32 48 64 128 256 512; do
    dir="$root/packaging/linux/icons/hicolor/${size}x${size}/apps"
    mkdir -p "$dir"
    rsvg-convert -w "$size" -h "$size" "$source" -o "$dir/zerotier-gui.png"
done

mkdir -p "$root/packaging/linux/icons/hicolor/scalable/apps"
cp "$source" "$root/packaging/linux/icons/hicolor/scalable/apps/zerotier-gui.svg"
rsvg-convert -w 48 -h 48 "$source" -o "$root/img/zerotier-gui.png"

echo "icons regenerated under packaging/linux/icons"
