#!/usr/bin/env bash
#
# make-deb.sh — baut das Linux-Release-Paket Prime-Photos-linux-x64.deb.
#
# Ablauf:
#   1. Binary nach usr/bin/prime-photos (0755) installieren
#   2. .desktop-Datei (packaging/linux/prime-photos.desktop) nach usr/share/applications/
#   3. Hicolor-Icons 16/32/64/128/256/512 aus dem iconset nach usr/share/icons/hicolor/
#      (das iconset rastert build.rs aus assets/logo.svg; 64 kommt aus icon_32x32@2x.png)
#   4. Lizenzen + copyright nach usr/share/doc/prime-photos/
#   5. DEBIAN/control schreiben; Depends aus `dpkg-shlibdeps`, ergänzt um die per
#      dlopen geladene Tray-Bibliothek (libayatana-appindicator3-1 | libappindicator3-1),
#      die dpkg-shlibdeps nicht sehen kann
#   6. `dpkg-deb --build --root-owner-group` nach <output>/Prime-Photos-linux-x64.deb
#
# Umgebungsvariablen:
#   VERSION=<x.y.z|vx.y.z>   Paketversion (Standard: version = "…" aus Cargo.toml)
#
# Aufruf:
#   packaging/make-deb.sh [--binary <datei>] [--iconset <dir>] [--output <dir>]
#
# Standard: --binary target/release/prime-photos
#           --iconset target/pp-appicon.iconset
#           --output  release/
#
# Hinweis: nur für Debian/Ubuntu (dpkg-deb, dpkg-shlibdeps); kein Codesigning —
# eine .deb-Signatur (debsig-verify/Repository) ist nicht Teil dieses Skripts.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary=""
iconset=""
output=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --binary) binary="$2"; shift 2 ;;
    --iconset) iconset="$2"; shift 2 ;;
    --output) output="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

binary="${binary:-$repo_root/target/release/prime-photos}"
iconset="${iconset:-$repo_root/target/pp-appicon.iconset}"
output="${output:-$repo_root/release}"

[[ -x "$binary" ]] || { echo "error: binary not executable: $binary" >&2; exit 1; }
[[ -d "$iconset" ]] || { echo "error: iconset dir not found: $iconset" >&2; exit 1; }

version="${VERSION:-}"
if [[ -z "$version" ]]; then
  version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo_root/Cargo.toml" | head -n1)"
fi
version="${version#v}"
[[ -n "$version" ]] || { echo "error: VERSION not set and Cargo.toml has no version" >&2; exit 1; }

arch="$(dpkg --print-architecture)"
package="prime-photos"
staging="$(mktemp -d)"
# Temporäre dpkg-shlibdeps-Wurzel (minimales debian/control) getrennt aufräumen.
shlibs_root="$(mktemp -d)"
cleanup() { rm -rf "$staging" "$shlibs_root"; }
trap cleanup EXIT

install -D -m 0755 "$binary" "$staging/usr/bin/prime-photos"
install -D -m 0644 "$repo_root/packaging/linux/prime-photos.desktop" "$staging/usr/share/applications/prime-photos.desktop"

install_icon() {
  install -D -m 0644 "$iconset/$1" "$staging/usr/share/icons/hicolor/${2}x${2}/apps/prime-photos.png"
}
install_icon icon_16x16.png 16
install_icon icon_32x32.png 32
install_icon icon_32x32@2x.png 64
install_icon icon_128x128.png 128
install_icon icon_256x256.png 256
install_icon icon_512x512.png 512

install -D -m 0644 "$repo_root/LICENSE-MIT" "$staging/usr/share/doc/prime-photos/LICENSE-MIT"
install -D -m 0644 "$repo_root/LICENSE-APACHE" "$staging/usr/share/doc/prime-photos/LICENSE-APACHE"

cat > "$staging/usr/share/doc/prime-photos/copyright" <<'EOF'
Prime Photos
Licensed under the MIT OR Apache-2.0 license, at your option.
See LICENSE-MIT and LICENSE-APACHE in this directory.
EOF

mkdir -p "$staging/DEBIAN"
cat > "$staging/DEBIAN/control" <<EOF
Package: $package
Version: $version
Section: graphics
Priority: optional
Architecture: $arch
Maintainer: Florian Reisinger <florian.reisinger.at@gmail.com>
Depends: libayatana-appindicator3-1 | libappindicator3-1
Recommends: gnome-keyring | kwalletmanager
Homepage: https://github.com/reisi007/prime-photos
Description: Back up photos from local folders to your own Amazon Photos account
 Prime Photos is a cross-platform desktop application that backs up photos
 from local folders to your own Amazon Photos account. It supports persistent
 backup targets, content-based deduplication and event-driven change detection.
EOF

# dpkg-shlibdeps braucht ein debian/-Verzeichnis; die Wurzel bleibt temporär und
# verlässt das Repo nicht.
mkdir -p "$shlibs_root/debian" "$shlibs_root/usr/bin"
cp "$binary" "$shlibs_root/usr/bin/prime-photos"
cat > "$shlibs_root/debian/control" <<EOF
Source: $package
Section: graphics
Priority: optional
Maintainer: Florian Reisinger <florian.reisinger.at@gmail.com>
Standards-Version: 4.6.2

Package: $package
Architecture: any
Depends: \${shlibs:Depends}, \${misc:Depends}
Description: test
 test
EOF

# Beide Ströme einsammeln: ohne sie endet ein Fehlschlag unter `set -e` stumm. Die
# Warnungen von dpkg-shlibdeps landen dabei zwischen den Zeilen — gesucht wird deshalb
# gezielt nach der `shlibs:Depends=`-Zeile (kein head/cut).
if ! shlibdeps_output="$(cd "$shlibs_root" && dpkg-shlibdeps -O -e./usr/bin/prime-photos 2>&1)"; then
  echo "error: dpkg-shlibdeps failed for $binary:" >&2
  printf '%s\n' "$shlibdeps_output" >&2
  exit 1
fi
shlibs="$(printf '%s\n' "$shlibdeps_output" | sed -n 's/^shlibs:Depends=//p')"
[[ -n "$shlibs" ]] || {
  echo "error: dpkg-shlibdeps produced no shlibs:Depends line for $binary:" >&2
  printf '%s\n' "$shlibdeps_output" >&2
  exit 1
}
sed -i "s#^Depends: .*#Depends: $shlibs, libayatana-appindicator3-1 | libappindicator3-1#" "$staging/DEBIAN/control"

mkdir -p "$output"
# mktemp legt das Staging mit 0700 an; der "./"-Eintrag des Archivs soll 0755 sein.
chmod 0755 "$staging"
# Der Dateiname ist fest auf linux-x64 gesetzt: es gibt genau ein Ubuntu-Bein (amd64),
# so kann er nicht unbemerkt von `dpkg --print-architecture` abweichen.
deb="$output/Prime-Photos-linux-x64.deb"
dpkg-deb --build --root-owner-group "$staging" "$deb"
echo "built: $deb"
