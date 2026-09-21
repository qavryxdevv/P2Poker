#!/bin/sh
# D-078: the Linux packages of a built client, from target/release/p2p-poker into dist/:
#
#   P2Poker-<version>-x86_64.AppImage       one file, for any distribution with glibc 2.35 or newer
#   p2poker_<version>_amd64.deb             Ubuntu, Debian, Mint, Pop!_OS, elementary, Zorin
#   p2poker-<version>-1.x86_64.rpm          Fedora, openSUSE, and the Red Hat family
#   P2Poker-<version>-linux-x86_64.tar.gz   the program and its menu entry, to run from where it is unpacked
#
#   sh tools/package-linux.sh 0.1.3
#
# The .deb and the .rpm install the program as /usr/bin/p2p-poker, a menu entry and an icon; the profile
# is in the user's data folder (~/.local/share/p2poker/profile), never under /usr (storage::profile).
#
# Needs dpkg-deb, rpmbuild, curl and sha256sum. The AppImage tool and the runtime it puts in front of the
# program are fetched at pinned versions and refused unless their SHA-256 is the one written here, the
# way the workflow pins every action to a commit: a moved release is not a way into this build.
set -eu

version="${1:?the version, as Cargo.toml says it}"
root=$(cd "$(dirname "$0")/.." && pwd)
bin="$root/target/release/p2p-poker"
desktop="$root/packaging/linux/p2poker.desktop"
icon="$root/assets/icon-256.png"
dist="$root/dist"
work="$root/target/package-linux"
test -x "$bin" || { echo "no program at $bin: build it first" >&2; exit 1; }

APPIMAGETOOL_URL="https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage"
APPIMAGETOOL_SHA256="ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0"
RUNTIME_URL="https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64"
RUNTIME_SHA256="2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d"

rm -rf "$work"
mkdir -p "$work" "$dist"

fetch() { # url sha256 file
  curl -fsSL --retry 3 -o "$3" "$1"
  echo "$2  $3" | sha256sum -c - >/dev/null || { echo "REFUSED: $1 is not the pinned file" >&2; exit 1; }
}

# The tree the .deb and the .rpm are made from.
stage="$work/stage"
install -Dm755 "$bin" "$stage/usr/bin/p2p-poker"
install -Dm644 "$desktop" "$stage/usr/share/applications/p2poker.desktop"
install -Dm644 "$icon" "$stage/usr/share/icons/hicolor/256x256/apps/p2poker.png"
install -Dm644 "$root/LICENSE" "$stage/usr/share/doc/p2poker/copyright"

summary="Decentralised poker: no house, no server, every card proven"
description="P2Poker is a No-Limit Texas Hold'em client with no server. Players find each other peer-to-peer and deal the cards themselves, with cryptographic shuffle proofs. Sit & Go tournaments for 2 to 10 players; play money only."

# .deb -- the libraries the window opens at run time, which no linker sees, are named by hand; ALSA's, which
# the sounds open the same way, is recommended rather than required: without it the client is silent, not broken
# (D-079). Ubuntu 24.04 renamed it libasound2t64, so either name will do.
deb="$work/deb"
cp -a "$stage" "$deb"
mkdir -p "$deb/DEBIAN"
size=$(du -sk "$deb/usr" | cut -f1)
cat > "$deb/DEBIAN/control" <<EOF
Package: p2poker
Version: $version
Architecture: amd64
Maintainer: P2Poker <270192017+qavryxdev@users.noreply.github.com>
Installed-Size: $size
Depends: libc6 (>= 2.35), libgl1, libegl1, libxkbcommon0, libxkbcommon-x11-0, libx11-6, libx11-xcb1, libxcursor1, libxrandr2, libxi6, libwayland-client0
Recommends: libasound2 | libasound2t64
Section: games
Priority: optional
Homepage: https://github.com/qavryxdevv/P2Poker
Description: $summary
 $description
EOF
dpkg-deb --root-owner-group --build "$deb" "$dist/p2poker_${version}_amd64.deb" >/dev/null

# .rpm -- the same tree, the same libraries by their sonames, which every rpm distribution provides.
rpmtop="$work/rpm"
mkdir -p "$rpmtop/SPECS"
cat > "$rpmtop/SPECS/p2poker.spec" <<EOF
Name: p2poker
Version: $version
Release: 1
Summary: $summary
License: GPL-3.0-or-later
URL: https://github.com/qavryxdevv/P2Poker
BuildArch: x86_64
Requires: libGL.so.1()(64bit), libEGL.so.1()(64bit), libxkbcommon.so.0()(64bit), libxkbcommon-x11.so.0()(64bit), libX11.so.6()(64bit), libX11-xcb.so.1()(64bit), libXcursor.so.1()(64bit), libXrandr.so.2()(64bit), libXi.so.6()(64bit), libwayland-client.so.0()(64bit)
Recommends: libasound.so.2()(64bit)
%define debug_package %{nil}
%define __strip /bin/true
%define _build_id_links none

%description
$description

%install
mkdir -p %{buildroot}
cp -a $stage/. %{buildroot}/

%files
/usr/bin/p2p-poker
/usr/share/applications/p2poker.desktop
/usr/share/icons/hicolor/256x256/apps/p2poker.png
/usr/share/doc/p2poker/copyright
EOF
rpmbuild --quiet --define "_topdir $rpmtop" --target x86_64 -bb "$rpmtop/SPECS/p2poker.spec"
cp "$rpmtop/RPMS/x86_64/p2poker-$version-1.x86_64.rpm" "$dist/"

# .tar.gz -- the program, its menu entry, its icon and its licence, in one folder.
tardir="$work/P2Poker-$version"
mkdir -p "$tardir"
cp "$bin" "$tardir/p2p-poker"
cp "$desktop" "$tardir/p2poker.desktop"
cp "$icon" "$tardir/p2poker.png"
cp "$root/LICENSE" "$tardir/LICENSE"
tar -C "$work" -czf "$dist/P2Poker-$version-linux-x86_64.tar.gz" "P2Poker-$version"

# AppImage -- the program behind the pinned runtime; the system's own graphics libraries are used, not
# carried, because the graphics driver is the system's.
appdir="$work/AppDir"
install -Dm755 "$bin" "$appdir/usr/bin/p2p-poker"
install -Dm644 "$desktop" "$appdir/usr/share/applications/p2poker.desktop"
install -Dm644 "$icon" "$appdir/usr/share/icons/hicolor/256x256/apps/p2poker.png"
cp "$desktop" "$appdir/p2poker.desktop"
cp "$icon" "$appdir/p2poker.png"
ln -s usr/bin/p2p-poker "$appdir/AppRun"
fetch "$APPIMAGETOOL_URL" "$APPIMAGETOOL_SHA256" "$work/appimagetool"
fetch "$RUNTIME_URL" "$RUNTIME_SHA256" "$work/runtime-x86_64"
chmod +x "$work/appimagetool"
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$work/appimagetool" --no-appstream --runtime-file "$work/runtime-x86_64" \
  "$appdir" "$dist/P2Poker-$version-x86_64.AppImage"

ls -l "$dist"
