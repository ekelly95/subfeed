#!/bin/sh
# Checks that subfeed's version is the same everywhere it's written down:
# CMakeLists.txt (the app), rust/Cargo.toml (the core), and the newest <release> in the metainfo
# (what app stores show). With a tag argument (v0.1.2), also checks that it names that version.
# Used by CI; run it before tagging a release.
set -eu
cd "$(dirname "$0")/.."

cmake=$(sed -n 's/^project(subfeed VERSION \([0-9.]*\).*/\1/p' CMakeLists.txt)
cargo=$(sed -n 's/^version = "\([0-9.]*\)"$/\1/p' rust/Cargo.toml | head -n 1)
metainfo=$(sed -n 's/.*<release version="\([0-9.]*\)".*/\1/p' io.github.ekelly95.Subfeed.metainfo.xml | head -n 1)
lock=$(sed -n '/^name = "subfeed_core"$/{n;s/^version = "\([0-9.]*\)"$/\1/p;}' rust/Cargo.lock)

echo "CMakeLists.txt $cmake, Cargo.toml $cargo, Cargo.lock $lock, metainfo $metainfo"
for v in "$cargo" "$lock" "$metainfo"; do
    if [ -z "$cmake" ] || [ "$v" != "$cmake" ]; then
        echo "Version mismatch: set all of them to the same version." >&2
        exit 1
    fi
done
if [ $# -gt 0 ] && [ "$1" != "v$cmake" ]; then
    echo "Tag $1 doesn't match version $cmake (expected v$cmake)." >&2
    exit 1
fi
echo "OK: $cmake"
