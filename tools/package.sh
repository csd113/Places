#!/bin/sh
# package.sh [--check-destination] [output-root]
#
# Builds a clean, self-contained Places distribution from a release binary.
#
# Layout produced (the assets/ + levels/ pair is the whole runtime payload):
#
#   <output-root>/Places/
#       places              the executable (built as `places`)
#       assets/             catalog, levels, models, textures, decals
#       levels/             drop-in compiled map packages (.placesmap)
#       README.md           the project README, for reference
#       THIRD_PARTY_LICENSES.txt
#
#   <output-root>/Places.app/          macOS bundle form of the same payload
#       Contents/Info.plist
#       Contents/MacOS/places
#       Contents/Resources/{assets,levels,...}
#
# The output root is an export parent shared with unrelated files: only the two
# product directories above are owned and replaced by this script. The root is
# canonicalised and refused when it is `/`, the home directory, the repository
# (or one of its ancestors). Publication is staged: the payload is built in a
# private directory and moved into place after the old products are removed,
# so a failure leaves the previous package (or the unrelated siblings) alone.
#
# `--check-destination` runs the destination validation only and exits 0/2; it
# never creates, deletes or copies anything. The regression suite uses it.
#
# The game resolves its asset root from the executable's own location
# (`$PLACES_ASSET_ROOT`, then the executable's directory and its ancestors,
# then a macOS bundle's Contents/Resources), so either form runs from any
# working directory and never reads the source tree.
set -eu

REPO=$(cd "$(dirname "$0")/.." && pwd)
OUT="$REPO/target/package"
CHECK_ONLY=0
while [ $# -gt 0 ]; do
    case "$1" in
    --check-destination) CHECK_ONLY=1 ;;
    -h | --help | --usage)
        sed -n '2,30p' "$0"
        exit 0
        ;;
    -*)
        echo "package.sh: unknown option '$1'" >&2
        exit 2
        ;;
    *)
        if [ "$OUT" != "$REPO/target/package" ]; then
            echo "package.sh: at most one output root" >&2
            exit 2
        fi
        OUT=$1
        ;;
    esac
    shift
done
BIN="$REPO/target/release/places"
NAME=places
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO/Cargo.toml" | head -1)

fail() {
    echo "package.sh: $1" >&2
    exit 2
}

# Absolute, physically-resolved form of a destination that need not exist yet.
# The deepest existing ancestor is resolved with `cd -P`/`pwd -P`, so a
# symlinked root (or symlinked ancestor) is compared as its real target and
# cannot disguise the home directory or the repository; the missing tail is
# appended lexically. Nothing is created.
absolute_path() {
    path=$1
    if [ -z "$path" ]; then
        fail "output root must not be empty"
    fi
    while [ "$path" != "/" ] && [ "${path%/}" != "$path" ]; do
        path=${path%/}
    done
    tail=""
    while [ ! -d "$path" ]; do
        base=$(basename "$path")
        path=$(dirname "$path")
        tail="/$base$tail"
    done
    resolved=$(cd -P "$path" && pwd -P)
    printf '%s%s' "$resolved" "$tail"
}

OUT_ABS=$(absolute_path "$OUT")
if [ "$OUT_ABS" = "/" ]; then
    fail "refusing to package into the filesystem root"
fi
if [ -z "$VERSION" ]; then
    fail "could not read the version from Cargo.toml"
fi
if [ -e "$OUT_ABS" ] && [ ! -d "$OUT_ABS" ]; then
    fail "output root exists and is not a directory: $OUT_ABS"
fi

# Refuse the repository, the home directory and any ancestor of either: the
# script owns only the two product directories below the root, and a root that
# contains the source or the user's files cannot be assumed disposable.
case "$REPO" in
"$OUT_ABS" | "$OUT_ABS"/*)
    fail "refusing to package into the repository or one of its ancestors: $OUT_ABS"
    ;;
esac
if [ -n "${HOME:-}" ] && [ -d "$HOME" ]; then
    HOME_ABS=$(cd -P "$HOME" && pwd -P)
    case "$HOME_ABS" in
    "$OUT_ABS" | "$OUT_ABS"/*)
        fail "refusing to package into the home directory or one of its ancestors: $OUT_ABS"
        ;;
    esac
fi

# Only real directories are replaced; a symlinked product could redirect the
# replacement outside the export root.
for product in Places Places.app; do
    target="$OUT_ABS/$product"
    if [ -L "$target" ]; then
        fail "$target is a symlink; refusing to replace it"
    fi
    if [ -e "$target" ] && [ ! -d "$target" ]; then
        fail "$target exists and is not a directory"
    fi
done

if [ "$CHECK_ONLY" = 1 ]; then
    echo "package.sh: destination is safe: $OUT_ABS"
    exit 0
fi

if [ ! -x "$BIN" ]; then
    echo "package.sh: $BIN not found; run 'cargo build --release' first" >&2
    exit 1
fi

echo "packaging Places $VERSION -> $OUT_ABS"
mkdir -p "$OUT_ABS"
STAGE=""
cleanup() {
    if [ -n "$STAGE" ]; then
        rm -rf "$STAGE"
    fi
}
trap cleanup EXIT HUP INT TERM
STAGE=$(mktemp -d "$OUT_ABS/.places-package-stage.XXXXXX")

# Build the whole payload in the private stage first; nothing below the root
# is touched until both products are complete.
mkdir -p "$STAGE/Places" "$STAGE/Places.app/Contents/MacOS" "$STAGE/Places.app/Contents/Resources"

# The flat distribution: executable beside its asset root.
cp "$BIN" "$STAGE/Places/$NAME"
cp "$REPO/README.md" "$REPO/THIRD_PARTY_LICENSES.txt" "$STAGE/Places/"
cp -R "$REPO/assets" "$STAGE/Places/assets"
mkdir -p "$STAGE/Places/levels"
# Only compiled packages are playable. Authoring sources stay in the repository;
# shipping one without its package would offer the player a row it cannot load.
cp "$REPO"/levels/*.placesmap "$STAGE/Places/levels/" 2>/dev/null || true

# The macOS bundle: the same payload under Contents/Resources.
cp "$BIN" "$STAGE/Places.app/Contents/MacOS/$NAME"
cat >"$STAGE/Places.app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>$NAME</string>
    <key>CFBundleIdentifier</key><string>io.github.csd113.places</string>
    <key>CFBundleName</key><string>Places</string>
    <key>CFBundleDisplayName</key><string>Places</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
cp -R "$REPO/assets" "$STAGE/Places.app/Contents/Resources/assets"
mkdir -p "$STAGE/Places.app/Contents/Resources/levels"
cp "$REPO"/levels/*.placesmap "$STAGE/Places.app/Contents/Resources/levels/" 2>/dev/null || true
cp "$REPO/README.md" "$REPO/THIRD_PARTY_LICENSES.txt" "$STAGE/Places.app/Contents/Resources/"

# Publish: replace exactly the two owned products, leaving every unrelated
# sibling in the export root untouched. `mv` within one directory is a rename.
rm -rf "$OUT_ABS/Places" "$OUT_ABS/Places.app"
mv "$STAGE/Places" "$OUT_ABS/Places"
mv "$STAGE/Places.app" "$OUT_ABS/Places.app"
rmdir "$STAGE"
STAGE=""
trap - EXIT HUP INT TERM

echo "  $OUT_ABS/Places/$NAME"
echo "  $OUT_ABS/Places/assets/catalog.json"
echo "  $OUT_ABS/Places.app/Contents/MacOS/$NAME"
echo "  $OUT_ABS/Places.app/Contents/Resources/assets/catalog.json"
