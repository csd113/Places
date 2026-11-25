#!/bin/sh
# package.sh [--check-destination] [--with-compiler | --with-editor] [--menu-package file] [--maps directory] [output-root]
#
# Stages a clean Places resource payload from a host release binary.
#
# Default exports contain no maps. --maps explicitly includes external compiled
# packages from one directory; it never fetches or compiles map sources.
# --menu-package supplies a current host-built menu export without changing the checkout.
# Resource layout (writable state is created in platform user storage):
#
#   <output-root>/Places/
#       places              the executable (built as `places`)
#       places-map-compiler      optional --with-compiler, same resource discovery
#       places-editor       optional --with-editor (includes compiler)
#       lib/                bundled SDL3 on macOS
#       assets/             catalog, gameplay profiles, levels, models, textures, decals
#       docs/INSTALLATION.md
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
# private directory and moved into place while old products remain recoverable,
# so a failure leaves the previous package (or the unrelated siblings) alone.
#
# `--check-destination` runs the destination validation only and exits 0/2; it
# never creates, deletes or copies anything. The regression suite uses it.
#
# All executables resolve assets beside themselves or in Contents/Resources.
# They never depend on the launch directory or write into this payload.
# macOS exports include SDL3 and ad-hoc signatures, not notarized installers.
set -eu

REPO=$(cd "$(dirname "$0")/.." && pwd)
OUT="$REPO/target/package"
CHECK_ONLY=0
MAPS=""
MENU_PACKAGE=""
WITH_COMPILER=0
WITH_EDITOR=0
while [ $# -gt 0 ]; do
    case "$1" in
    --check-destination) CHECK_ONLY=1 ;;
    --with-compiler) WITH_COMPILER=1 ;;
    --with-editor) WITH_EDITOR=1; WITH_COMPILER=1 ;;
    --menu-package)
        shift
        if [ $# -eq 0 ] || [ -z "$1" ] || [ -n "$MENU_PACKAGE" ]; then
            echo 'package.sh: --menu-package needs one file and may be used once' >&2
            exit 2
        fi
        MENU_PACKAGE=$1
        ;;
    --maps)
        shift
        if [ $# -eq 0 ] || [ -n "$MAPS" ]; then
            echo 'package.sh: --maps needs one directory and may be used once' >&2
            exit 2
        fi
        MAPS=$1
        ;;
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
# Every export validates its application-owned menu, even with zero gameplay maps.
[ -x "$REPO/target/release/places-map-compiler" ] || fail "menu validation needs the matching release places-map-compiler binary"
if [ "$WITH_COMPILER" = 1 ]; then
    [ -x "$REPO/target/release/places-map-compiler" ] || fail "release places-map-compiler not found; build the workspace first"
fi
if [ "$WITH_EDITOR" = 1 ]; then
    [ -x "$REPO/target/release/places-editor" ] || fail "release places-editor not found; build the workspace first"
fi

if [ -n "$MAPS" ]; then
    [ -d "$MAPS" ] || fail "map directory does not exist: $MAPS"
    [ -x "$REPO/target/release/places-map-compiler" ] || fail "--maps validation needs the release places-map-compiler binary"
    MAPS=$(cd -P "$MAPS" && pwd -P)
fi
if [ -n "$MENU_PACKAGE" ]; then
    [ -f "$MENU_PACKAGE" ] || fail "--menu-package must be an existing project export"
fi

echo "packaging Places $VERSION -> $OUT_ABS"
mkdir -p "$OUT_ABS"
STAGE=""
PUBLISHING=0
cleanup() {
    if [ -n "$STAGE" ]; then
        if [ "$PUBLISHING" = 1 ]; then
            # A failed publication restores both previous products. A missing
            # staged product means its new version was moved into the root.
            for product in Places Places.app; do
                if [ ! -d "$STAGE/$product" ]; then
                    rm -rf "$OUT_ABS/$product"
                fi
                if [ -d "$STAGE/previous/$product" ]; then
                    mv "$STAGE/previous/$product" "$OUT_ABS/$product"
                fi
            done
        fi
        rm -rf "$STAGE"
    fi
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
STAGE=$(mktemp -d "$OUT_ABS/.places-package-stage.XXXXXX")

# Validate the final resource tree. The CLI reports dependency changes as
# data (not as an error), so require a true dependency result explicitly.
verify_staged_menu() (
    assets=$1
    menu="$assets/shared/menu/scene.bin"
    state=$(mktemp -d "$STAGE/.menu-state.XXXXXX")
    for key in $(env | sed -n 's/^\(PLACES_[A-Za-z0-9_]*\)=.*/\1/p'); do unset "$key"; done
    export PLACES_STATE_ROOT="$state" PLACES_CONFIG_ROOT="$state" PLACES_CACHE_ROOT="$state/cache"
    compiler="$REPO/target/release/places-map-compiler"
    report=$("$compiler" validate "$menu" --asset-root "$assets" --json)
    printf '%s\n' "$report" | python3 -c 'import json, sys; sys.exit(None if json.load(sys.stdin)["dependencies_intact"] is True else "Bundled menu dependencies are not intact")'
    "$compiler" verify "$REPO/projects/menu" --level places_menu --package "$menu" \
        --asset-root "$assets" --require-current --json > /dev/null
    echo "Bundled menu validated against final payload and current source"
)

# Build the whole payload in the private stage first; nothing below the root
# is touched until both products are complete.
mkdir -p "$STAGE/Places" "$STAGE/Places.app/Contents/MacOS" "$STAGE/Places.app/Contents/Resources"

# The flat distribution: executable beside its asset root.
cp "$BIN" "$STAGE/Places/$NAME"
if [ "$WITH_COMPILER" = 1 ]; then
    cp "$REPO/target/release/places-map-compiler" "$STAGE/Places/places-map-compiler"
fi
if [ "$WITH_EDITOR" = 1 ]; then
    cp "$REPO/target/release/places-editor" "$STAGE/Places/places-editor"
fi
cp "$REPO/README.md" "$STAGE/Places/README.md"
cp "$REPO/LICENSE" "$REPO/THIRD_PARTY_LICENSES.txt" "$STAGE/Places/"
mkdir -p "$STAGE/Places/assets/levels" "$STAGE/Places/docs"
cp "$REPO/docs/INSTALLATION.md" "$STAGE/Places/docs/"
if [ "$WITH_EDITOR" = 1 ]; then
    cp "$REPO/docs/EDITOR.md" "$STAGE/Places/docs/"
fi
cp "$REPO/assets/catalog.json" "$REPO/assets/gameplay.json" "$STAGE/Places/assets/"
# Sound licenses and exact provenance accompany the compact runtime library.
cp -R "$REPO/assets/audio" "$STAGE/Places/assets/audio"
if [ "$WITH_EDITOR" = 1 ]; then
    cp -R "$REPO/assets/editor" "$STAGE/Places/assets/editor"
fi
# References and master source artwork belong to authoring, not runtime exports.
for directory in shared environments entities diagnostic; do
    cp -R "$REPO/assets/$directory" "$STAGE/Places/assets/$directory"
done
rm -rf "$STAGE/Places/assets/shared/branding"
find "$STAGE/Places/assets" -type f \( -name '.DS_Store' -o -name '*_master.png' -o -name '*.blend' -o -name '*.blend1' -o -name '*.xcf' -o -name 'CABINET_ARTWORK_PROMPTS.md' \) -delete
if [ -n "$MENU_PACKAGE" ]; then
    cp -- "$MENU_PACKAGE" "$STAGE/Places/assets/shared/menu/scene.bin"
fi
verify_staged_menu "$STAGE/Places/assets"
if [ -n "$MAPS" ]; then
    python3 "$REPO/release/export_maps.py" "$MAPS" "$STAGE/Places" \
        "$REPO/target/release/places-map-compiler"
fi

# The macOS bundle: the same payload under Contents/Resources.
cp "$BIN" "$STAGE/Places.app/Contents/MacOS/$NAME"
if [ "$WITH_COMPILER" = 1 ]; then
    cp "$REPO/target/release/places-map-compiler" "$STAGE/Places.app/Contents/MacOS/places-map-compiler"
fi
if [ "$WITH_EDITOR" = 1 ]; then
    cp "$REPO/target/release/places-editor" "$STAGE/Places.app/Contents/MacOS/places-editor"
fi
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
cp -R "$STAGE/Places/assets" "$STAGE/Places.app/Contents/Resources/assets"
cp -R "$STAGE/Places/docs" "$STAGE/Places.app/Contents/Resources/docs"
cp "$STAGE/Places/README.md" "$REPO/LICENSE" "$REPO/THIRD_PARTY_LICENSES.txt" "$STAGE/Places.app/Contents/Resources/"

verify_staged_menu "$STAGE/Places.app/Contents/Resources/assets"
if [ "$(uname -s)" = Darwin ]; then
    python3 "$REPO/release/bundle-macos.py" "$STAGE/Places" "$STAGE/Places.app"
fi

# Publish: replace exactly the two owned products, leaving every unrelated
# sibling in the export root untouched. `mv` within one directory is a rename.
mkdir "$STAGE/previous"
PUBLISHING=1
for product in Places Places.app; do
    if [ -d "$OUT_ABS/$product" ]; then
        mv "$OUT_ABS/$product" "$STAGE/previous/$product"
    fi
done
mv "$STAGE/Places" "$OUT_ABS/Places"
mv "$STAGE/Places.app" "$OUT_ABS/Places.app"
PUBLISHING=0
rm -rf "$STAGE"
STAGE=""
trap - EXIT HUP INT TERM

echo "  $OUT_ABS/Places/$NAME"
echo "  $OUT_ABS/Places/assets/catalog.json"
echo "  $OUT_ABS/Places.app/Contents/MacOS/$NAME"
echo "  $OUT_ABS/Places.app/Contents/Resources/assets/catalog.json"
