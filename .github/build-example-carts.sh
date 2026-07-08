#!/usr/bin/env bash
# Export every example cart to a shareable PNG, ready to drop onto a handheld's SD card.
#
# Each cart PNG is what the pixel8 player loads directly, so the output directory mirrors the
# handheld bundle's pixel8/carts folder and can be copied straight onto the device:
#
#   ./.github/build-example-carts.sh [output-dir]   # defaults to dist/handheld/pixel8/carts
set -euo pipefail

out="${1:-dist/handheld/pixel8/carts}"
root="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$out"

# pixel8 export needs no audio, so build the smaller no-default-features exporter.
# A debug build suffices: the exporter only drives the carts' own --release wasm
# builds, so its opt level doesn't change the exported PNGs — and it compiles faster.
echo "building the pixel8 exporter..."
cargo build -p pixel8-console --no-default-features
pixel8="$root/target/debug/pixel8"

for manifest in "$root"/examples/*/Cargo.toml; do
  ex=$(basename "$(dirname "$manifest")")
  echo "exporting $ex..."
  "$pixel8" export "$root/examples/$ex" "$out/$ex.png"
done

echo "exported example carts to $out:"
ls -la "$out"
