#!/bin/sh
# package.sh [output-root]
#
# Builds a clean, self-contained Places distribution from a release binary.
#
# Layout produced (the assets/ + levels/ pair is the whole runtime payload):
#
#   <output-root>/Places/
#       places              the executable (built as `liminal-rust`)
#       assets/             catalog, levels, models, textures, decals
#       levels/             level packs and drop-in custom levels
#       README.md           the project README, for reference
#       THIRD_PARTY_LICENSES.txt
#
#   <output-root>/Places.app/          macOS bundle form of the same payload
#       Contents/Info.plist
#       Contents/MacOS/places
#       Contents/Resources/{assets,levels,...}
#
# The game resolves its asset root from the executable's own location
# (`$LIMINAL_ASSET_ROOT`, then the executable's directory and its ancestors,
# then a macOS bundle's Contents/Resources), so either form runs from any
# working directory and never reads the source tree.
set -eu

REPO=$(cd "$(dirname "$0")/.." && pwd)
OUT=${1:-"$REPO/target/package"}
BIN="$REPO/target/release/liminal-rust"
NAME=places
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO/Cargo.toml" | head -1)

if [ ! -x "$BIN" ]; then
    echo "package.sh: $BIN not found; run 'cargo build --release' first" >&2
    exit 1
fi

echo "packaging Places $VERSION -> $OUT"
rm -rf "$OUT"
mkdir -p "$OUT/Places" "$OUT/Places.app/Contents/MacOS" "$OUT/Places.app/Contents/Resources"

# The flat distribution: executable beside its asset root.
cp "$BIN" "$OUT/Places/$NAME"
cp "$REPO/README.md" "$REPO/THIRD_PARTY_LICENSES.txt" "$OUT/Places/"
cp -R "$REPO/assets" "$OUT/Places/assets"
mkdir -p "$OUT/Places/levels"
cp "$REPO"/levels/*.json "$OUT/Places/levels/" 2>/dev/null || true

# The macOS bundle: the same payload under Contents/Resources.
cp "$BIN" "$OUT/Places.app/Contents/MacOS/$NAME"
cat > "$OUT/Places.app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>$NAME</string>
    <key>CFBundleIdentifier</key><string>io.vitrallis.liminalrust</string>
    <key>CFBundleName</key><string>Places</string>
    <key>CFBundleDisplayName</key><string>Places</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
cp -R "$REPO/assets" "$OUT/Places.app/Contents/Resources/assets"
mkdir -p "$OUT/Places.app/Contents/Resources/levels"
cp "$REPO"/levels/*.json "$OUT/Places.app/Contents/Resources/levels/" 2>/dev/null || true
cp "$REPO/README.md" "$REPO/THIRD_PARTY_LICENSES.txt" "$OUT/Places.app/Contents/Resources/"

echo "  $OUT/Places/$NAME"
echo "  $OUT/Places/assets/catalog.json"
echo "  $OUT/Places.app/Contents/MacOS/$NAME"
echo "  $OUT/Places.app/Contents/Resources/assets/catalog.json"
