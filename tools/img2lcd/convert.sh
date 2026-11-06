#!/usr/bin/env bash
# Regenerates firmware/nxt-lcd/src/ferris.rs from the committed SVG.
# Needs rsvg-convert + ImageMagick (`nix-shell -p librsvg imagemagick`) and
# rustc. The generated file is committed, so a normal firmware build does
# not need any of this.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
svg="$root/firmware/nxt-lcd/assets/rustacean-flat-happy.svg"
out="$root/firmware/nxt-lcd/src/ferris.rs"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Ferris is 3:2, so 64 rows high is 96 wide; letterbox to the 100-column
# panel. A hard threshold keeps the silhouette, eyes and claws readable;
# dithering turned the orange body into noise.
rsvg-convert -w 96 -h 64 -b white "$svg" -o "$work/ferris.png"
magick "$work/ferris.png" -background white -gravity center -extent 100x64 \
    -colorspace Gray -threshold 55% -compress none "pbm:$work/ferris.pbm"

rustc -O "$root/tools/img2lcd/img2lcd.rs" -o "$work/img2lcd"
"$work/img2lcd" "$work/ferris.pbm" FERRIS > "$out"
echo "wrote $out"
