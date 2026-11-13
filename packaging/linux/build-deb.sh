#!/bin/sh
# Build an amd64 .deb for the Rust/GPUI ZeroTier-GUI.
# Run `cargo build --release` first.
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
arch=${DEB_ARCH:-$(dpkg --print-architecture)}
binary="$root/target/release/zerotier-gui"

if [ ! -x "$binary" ]; then
    echo "error: $binary not found; run 'cargo build --release' first" >&2
    exit 1
fi

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

mkdir -p \
    "$stage/DEBIAN" \
    "$stage/usr/bin" \
    "$stage/usr/share/applications" \
    "$stage/usr/share/doc/zerotier-gui" \
    "$stage/usr/share/icons" \
    "$stage/usr/share/pixmaps"

install -m755 "$binary" "$stage/usr/bin/zerotier-gui"
sed 's|^Exec=.*|Exec=/usr/bin/zerotier-gui|' \
    packaging/linux/zerotier-gui.desktop \
    > "$stage/usr/share/applications/zerotier-gui.desktop"

# Full hicolor theme so GNOME, KDE, Xfce and docks can find the icon.
cp -a packaging/linux/icons/hicolor "$stage/usr/share/icons/"
install -m644 packaging/linux/icons/hicolor/256x256/apps/zerotier-gui.png \
    "$stage/usr/share/pixmaps/zerotier-gui.png"

install -m644 LICENSE NOTICE README.md "$stage/usr/share/doc/zerotier-gui/"
install -m644 packaging/linux/debian-copyright \
    "$stage/usr/share/doc/zerotier-gui/copyright"

find "$stage/usr/share/icons" -type d -exec chmod 755 {} +
find "$stage/usr/share/icons" -type f -exec chmod 644 {} +

size=$(du -sk "$stage" | cut -f1)

cat > "$stage/DEBIAN/control" <<EOF
Package: zerotier-gui
Version: $version
Architecture: $arch
Maintainer: Senasgr <39322822+senasgr-eth@users.noreply.github.com>
Installed-Size: $size
Depends: libxkbcommon0, libxkbcommon-x11-0, libxcb1, policykit-1
Recommends: zerotier-one
Section: net
Priority: optional
Homepage: https://github.com/senasgr-eth/zerotier-gui-rs
Description: Unofficial Rust + GPUI front-end for ZeroTier
 Manage ZeroTier networks, peers and connection paths.
 .
 This is a cross-platform rewrite of the original ZeroTier-GUI
 using Rust and GPUI. Service and interface management are
 available on Linux via systemd and polkit.
EOF

cat > "$stage/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t /usr/share/icons/hicolor >/dev/null 2>&1 || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database /usr/share/applications >/dev/null 2>&1 || true
fi
exit 0
EOF

cat > "$stage/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t /usr/share/icons/hicolor >/dev/null 2>&1 || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database /usr/share/applications >/dev/null 2>&1 || true
fi
exit 0
EOF

chmod 755 "$stage/DEBIAN/postinst" "$stage/DEBIAN/postrm"

mkdir -p "$root/dist"
output="$root/dist/zerotier-gui_${version}_${arch}.deb"
dpkg-deb --build --root-owner-group "$stage" "$output"
echo "built $output"
