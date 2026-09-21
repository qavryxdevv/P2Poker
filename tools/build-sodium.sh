#!/bin/sh
# D-078: libsodium for a build that is not MSVC's -- static, from the vendored source. The counterpart of
# tools/build-tox.ps1, which builds it with MSVC on Windows.
#
# The vendored tree is libsodium's git tree, which does not carry the generated `configure`. So it is copied
# under target/ and set up there with this system's autotools -- `autogen.sh -s -b`: -s makes the scripts,
# -b keeps autogen from fetching config.guess and config.sub from the network -- and the vendored tree
# stays byte for byte what upstream published. build.rs links target/libsodium/src/libsodium/.libs/libsodium.a.
#
# Needs: a C compiler, make, autoconf, automake and libtool. Run once; a second run keeps what is built.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
work="$root/target/libsodium"
lib="$work/src/libsodium/.libs/libsodium.a"
if [ -f "$lib" ]; then
  echo "libsodium.a is there: $lib"
  exit 0
fi
rm -rf "$work"
mkdir -p "$root/target"
cp -R "$root/vendor/libsodium" "$work"
cd "$work"
sh ./autogen.sh -s -b
sh ./configure --disable-shared --enable-static --with-pic --disable-dependency-tracking --quiet
make -j"$(nproc 2>/dev/null || echo 2)" >/dev/null
test -f "$lib"
echo "built: $lib"
