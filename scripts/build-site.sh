#!/usr/bin/env bash
# Build the whole GitHub Pages site: the tutorial book at the site root and
# every example cart under play/ — exported, verified and web-exported, with
# the cart-shelf index page and redirects from the carts' old root URLs.
# Used by .github/workflows/deploy-site.yml, and just as happy locally:
#
#   ./scripts/build-site.sh site
#   python3 -m http.server -d site
#
# Requires mdbook and the wasm32-unknown-unknown target.
set -euo pipefail

site="${1:?usage: build-site.sh <site-dir>}"
# Resolve the output dir against the caller's cwd before moving to the repo
# root (export-web must run from the source tree to find pixel8-web).
case "$site" in /*) ;; *) site="$(pwd)/$site" ;; esac
cd "$(dirname "$0")/.."

# The carts to publish, in shelf order. The shelf page's descriptions live
# with its markup in scripts/build-index.sh — keep the two lists in sync.
carts=(sprite_move platformer sfx_demo music_demo)

# The tutorial book is the site root.
mdbook build book
mkdir -p "$site"
cp -r book/book/. "$site/"

# Headless export tools only: --no-default-features skips the audio backend,
# so no ALSA system packages are needed. A debug build is fine and much
# faster to compile — the console only orchestrates the carts' own --release
# wasm builds and the web player's web-release build, so its own opt level
# doesn't affect the exported artifacts.
cargo build -p pixel8-console --no-default-features

# Console and editor screenshots for the book's console chapter, taken
# headless with the platformer example loaded.
./target/debug/pixel8 snap examples/platformer "$site/shots"

# The book's chapters embed these exports as iframes (play/<cart>.html).
mkdir -p "$site/play"
for cart in "${carts[@]}"; do
  ./target/debug/pixel8 export "examples/$cart" "$site/play/$cart.png"
  ./target/debug/pixel8 verify "$site/play/$cart.png"
  ./target/debug/pixel8 export-web "$site/play/$cart.png" "$site/play/$cart.html"
done

./scripts/build-index.sh "$site/play"

# The carts used to live at the site root; keep the old links alive now that
# the book sits there.
for cart in "${carts[@]}"; do
  cat > "$site/$cart.html" <<EOF
<!doctype html>
<meta charset="utf-8">
<meta http-equiv="refresh" content="0; url=play/$cart.html">
<link rel="canonical" href="play/$cart.html">
<p>Moved to <a href="play/$cart.html">play/$cart.html</a>.</p>
EOF
done

echo "built $site"
