#!/bin/sh
# S1-JA: does a software centre list the installed package, so that a player can remove it where they found it?
# GNOME Software and KDE Discover list the components the system's AppStream knows, not the packages dpkg knows;
# the .deb of 0.1.4 installed a program neither of them showed. Run on a Linux runner once the .deb is installed
# (.github/workflows/release.yml and repackage-linux.yml), with AppStream's appstreamcli and rpm at hand:
#
#   sh tools/check-linux-listing.sh dist/p2poker-0.1.4-1.x86_64.rpm
#
# 1. AppStream's own validator reads the tree the .deb and the .rpm are made from, the menu entry included;
# 2. dpkg says the package p2poker installed the metainfo, the menu entry it names and the program;
# 3. the system's AppStream -- what the software centres read -- knows the component by its id, as a desktop
#    application started by that menu entry, with that program as its binary;
# 4. the .rpm carries the same file.
set -eu

id=io.github.qavryxdevv.P2Poker
rpm="${1:?the .rpm to look into}"
root=$(cd "$(dirname "$0")/.." && pwd)
stage="$root/target/package-linux/stage"
meta="/usr/share/metainfo/$id.metainfo.xml"
command -v appstreamcli >/dev/null || { echo "no appstreamcli here: AppStream's package is 'appstream'" >&2; exit 1; }

echo "== AppStream's validator, on the tree the packages are made from"
appstreamcli validate-tree --no-net "$stage"

echo "== what dpkg says the package installed"
for f in "$meta" /usr/share/applications/p2poker.desktop /usr/bin/p2p-poker; do
  owner=$(dpkg -S "$f" 2>/dev/null | cut -d: -f1) || owner=
  [ "$owner" = p2poker ] || { echo "dpkg does not say the package p2poker installed $f" >&2; exit 1; }
  echo "p2poker: $f"
done

echo "== what the system's AppStream knows"
sudo -n appstreamcli refresh-cache --force >/dev/null 2>&1 || echo "(the cache was not refreshed; read as it is)"
dump=$(appstreamcli dump "$id") || { echo "the system's AppStream does not know $id" >&2; exit 1; }
for want in 'type="desktop-application"' '<launchable type="desktop-id">p2poker.desktop</launchable>' \
  '<binary>p2p-poker</binary>'; do
  case "$dump" in
    *"$want"*) echo "has $want" ;;
    *) echo "the system's AppStream knows $id without $want:" >&2; echo "$dump" >&2; exit 1 ;;
  esac
done
appstreamcli get "$id"

echo "== the .rpm"
rpm -qlp "$rpm" | grep -qx "$meta" || { echo "$rpm does not carry $meta" >&2; exit 1; }
echo "$rpm carries $meta"

echo "listed: $id, started by p2poker.desktop, in the .deb and in the .rpm"
