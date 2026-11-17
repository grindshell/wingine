#!/usr/bin/env bash
#
# Builds a game for the web into a folder to serve: its module, prepared for the page by
# wasm-bindgen as `pkg/game.js`, beside the page and its WASI functions. See docs/web.md.
#
#   web/build.sh                        the RPG, optimized, into web/dist
#   web/build.sh --game examples/pong   another game's folder, embedded by web/player
#   web/build.sh --dev                  faster to build, slower to play
#   web/build.sh --title "Pong"         the page's title before the game starts
#   web/build.sh --manifest ../my-game/web/Cargo.toml --package my-game-web --out dist/web
#                                       a module built by a crate of another workspace
#   web/build.sh --page path/to/page    another page, which imports ./pkg/game.js
#
# Serve the folder over HTTP, such as with `python -m http.server 8737 --directory web/dist`.
#
# Needs WASI SDK (see wasm-env.sh), the wasm32-unknown-unknown target, and wasm-bindgen-cli at the
# version of wasm-bindgen the build locks. On Windows, run it from Git Bash.

set -euo pipefail

die() { echo "build.sh: $*" >&2; exit 1; }

web=$(cd "$(dirname "$0")" && pwd)
manifest=$web/Cargo.toml
package=wingine-player
out=$web/dist
page=$web/page
title=Wingine
profile=release
while [ $# -gt 0 ]; do
  case $1 in
    --dev) profile=dev ;;
    --game) [ $# -ge 2 ] || die "--game needs a folder"; WINGINE_GAME=$(cd "$2" && pwd); shift ;;
    --manifest) [ $# -ge 2 ] || die "--manifest needs a path"; manifest=$2; shift ;;
    --package) [ $# -ge 2 ] || die "--package needs a name"; package=$2; shift ;;
    --out) [ $# -ge 2 ] || die "--out needs a folder"; out=$2; shift ;;
    --page) [ $# -ge 2 ] || die "--page needs a folder"; page=$2; shift ;;
    --title) [ $# -ge 2 ] || die "--title needs a title"; title=$2; shift ;;
    *) die "unknown option $1: see the comment at the top of build.sh" ;;
  esac
  shift
done

# shellcheck source=wasm-env.sh
. "$web/wasm-env.sh"
if [ -n "${WINGINE_GAME:-}" ]; then
  case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*) WINGINE_GAME=$(cygpath -m "$WINGINE_GAME") ;;
  esac
  export WINGINE_GAME
fi

command -v wasm-bindgen >/dev/null || die "wasm-bindgen-cli isn't installed"
locked=$(cargo pkgid --manifest-path "$manifest" wasm-bindgen)
locked=${locked##*@}
have=$(wasm-bindgen --version)
have=${have##* }
[ "$locked" = "$have" ] ||
  die "wasm-bindgen-cli is $have, but the build locks wasm-bindgen $locked: cargo install wasm-bindgen-cli --version $locked"

args=(build --manifest-path "$manifest" -p "$package" --target wasm32-unknown-unknown)
folder=debug
if [ "$profile" = release ]; then
  args+=(--release)
  folder=release
fi
cargo "${args[@]}"

# Where Cargo put the module, from its metadata's `"target_directory":"..."`, whose backslashes
# on Windows are escaped.
target=$(cargo metadata --format-version 1 --no-deps --manifest-path "$manifest" |
  grep -o '"target_directory":"[^"]*"' | sed 's/^"target_directory":"//; s/"$//; s/\\\\/\//g')
module=$target/wasm32-unknown-unknown/$folder/${package//-/_}.wasm
[ -f "$module" ] || die "no module at $module: is $package a cdylib?"

mkdir -p "$out"
rm -rf "$out/pkg"
wasm-bindgen --target web --no-typescript --out-dir "$out/pkg" --out-name game "$module"
cp -R "$page"/. "$out"/
if [ -f "$out/index.html" ]; then
  # The title as HTML, then escaped for sed's replacement.
  html=$(printf '%s' "$title" | sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g; s/"/\&quot;/g')
  html=$(printf '%s' "$html" | sed 's/[\\|&]/\\&/g')
  sed "s|__TITLE__|$html|g" "$out/index.html" >"$out/index.html.tmp"
  mv "$out/index.html.tmp" "$out/index.html"
fi
size=$(wc -c <"$out/pkg/game_bg.wasm")
echo "Built $out ($((size / 1024 / 1024)) MB module)"
