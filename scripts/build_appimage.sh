#!/usr/bin/env bash
# Builds the Linux AppImage of prepolix: release build, Gmsh library and the system libraries it
# needs (via linuxdeploy), desktop entry, icon, .plx file type and license texts, packed with
# appimagetool.
#
#     scripts/build_appimage.sh
#
# Needs (Debian/Ubuntu): sudo apt install libasound2-dev libglu1-mesa libxft2 libxcursor1 \
#                            libxinerama1 libgomp1 python3 curl file zsync
# The AppImage runs on systems with a glibc at least as new as the build system's, so releases
# are built on the oldest supported Ubuntu runner.
#
# The AppImage ends up in target/appimage/prepolix-<version>-x86_64.AppImage, next to its .zsync
# file for updates.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
out=target/appimage
appdir="$out/AppDir"
tools="$out/tools"
gmsh=target/gmsh
# Pinned so a release does not change with the tools' rolling builds.
linuxdeploy_url=https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage
appimagetool_url=https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
# Lets the tools run without FUSE, e.g. in containers and on CI runners.
export APPIMAGE_EXTRACT_AND_RUN=1

for tool in python3 curl file; do
    command -v "$tool" >/dev/null || {
        echo "$tool fehlt, siehe Kopf von $0" >&2
        exit 1
    }
done

version="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)"
[ -n "$version" ] || { echo "Version in Cargo.toml nicht gefunden" >&2; exit 1; }

cargo build --release --locked -p plx-app
[ -f "$gmsh/libgmsh.so.4.15" ] || python3 scripts/fetch_gmsh.py --platform linux --dest "$gmsh"

mkdir -p "$tools"
for entry in "linuxdeploy $linuxdeploy_url" "appimagetool $appimagetool_url"; do
    set -- $entry
    [ -x "$tools/$1" ] || { curl -sSfL -o "$tools/$1" "$2" && chmod +x "$tools/$1"; }
done

rm -rf "$appdir"
# The icon file names give the icon's name, which must match Icon= in the desktop entry.
icons="$out/icons"
mkdir -p "$icons"
cp crates/plx-app/assets/icon/prepolix.png "$icons/prepolix.png"
cp assets/icon/prepolix.svg "$icons/prepolix.svg"
# Gmsh is loaded at run time, so linuxdeploy only finds it when named; it bundles the
# libraries Gmsh links against except those every desktop system has (libGL, X11, glibc ...)
# and points the executable's RUNPATH at usr/lib, where the loader then finds libgmsh. It also
# copies the Debian copyright files of the bundled libraries to usr/share/doc/<package>.
"$tools/linuxdeploy" --appdir "$appdir" \
    --executable target/release/prepolix \
    --library "$gmsh/libgmsh.so.4.15" \
    --desktop-file installer/linux/prepolix.desktop \
    --icon-file "$icons/prepolix.png" --icon-file "$icons/prepolix.svg"

install -Dm644 installer/linux/prepolix-mime.xml "$appdir/usr/share/mime/packages/prepolix.xml"

doc="$appdir/usr/share/doc/prepolix"
mkdir -p "$doc/licenses"
cp LICENSE "$doc/LICENSE.txt"
cp installer/linux/THIRD-PARTY.txt "$doc/"
cp "$gmsh/GMSH-LICENSE.txt" "$gmsh/GMSH-CREDITS.txt" installer/windows/licenses/*.txt "$doc/licenses/"
cp crates/plx-app/assets/fonts/OFL.txt "$doc/licenses/NotoSans-OFL.txt"
commit="$(git rev-parse HEAD 2>/dev/null || echo unbekannt)"
dirty="$(git status --porcelain --untracked-files=no 2>/dev/null | head -1)"
{
    echo "PrePoLix $version"
    echo "Source: https://github.com/KwentiN-ui/prepolix"
    echo "Commit: $commit${dirty:+ (with local changes)}"
} >"$doc/SOURCE.txt"

appimage="$out/prepolix-$version-x86_64.AppImage"
rm -f "$appimage" "$appimage.zsync"
# Update information for AppImageUpdate, Gear Lever and similar tools: they look for the newest
# .zsync file in the latest GitHub release and download only the changed blocks. appimagetool
# writes the .zsync file into the current directory if zsyncmake is installed; it is moved next
# to the AppImage, and both go to the release.
update_info="gh-releases-zsync|KwentiN-ui|PrePoLix|latest|prepolix-*-x86_64.AppImage.zsync"
command -v zsyncmake >/dev/null || echo "Warnung: zsyncmake fehlt (apt install zsync), keine .zsync-Datei" >&2
ARCH=x86_64 VERSION="$version" "$tools/appimagetool" --no-appstream \
    --updateinformation "$update_info" "$appdir" "$appimage"
zsync="$(basename "$appimage").zsync"
[ ! -f "$zsync" ] || mv "$zsync" "$out/"
echo "AppImage: $appimage ($(du -h "$appimage" | cut -f1))"
