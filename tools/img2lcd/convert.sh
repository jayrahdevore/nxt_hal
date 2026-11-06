#!/usr/bin/env bash
# Regenerates the committed LCD bitmaps from their SVGs:
#   firmware/nxt-lcd/assets/rustacean-flat-happy.svg  -> firmware/nxt-lcd/src/ferris.rs
#   firmware/nxt-boot/assets/wrench.svg               -> firmware/nxt-boot/src/wrench.rs
# Needs rsvg-convert + ImageMagick (`nix-shell -p librsvg imagemagick`) and
# rustc. The generated files are committed, so a normal firmware build does
# not need any of this.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

rustc --edition 2021 -O "$root/tools/img2lcd/img2lcd.rs" -o "$work/img2lcd"

# convert <svg, relative to root> <generated .rs, relative to root> <STATIC_NAME>
#
# Both SVGs are 3:2, so 64 rows high is 96 wide; letterbox to the 100-column
# panel. A hard threshold keeps silhouettes, eyes and cut-outs readable;
# dithering turned Ferris's orange body into noise.
convert() {
    local svg="$root/$1" out="$root/$2" name="$3"
    rsvg-convert -w 96 -h 64 -b white "$svg" -o "$work/$name.png"
    magick "$work/$name.png" -background white -gravity center -extent 100x64 \
        -colorspace Gray -threshold 55% -compress none "pbm:$work/$name.pbm"
    "$work/img2lcd" "$work/$name.pbm" "$name" "$1" > "$out"
    echo "wrote $out"
}

convert firmware/nxt-lcd/assets/rustacean-flat-happy.svg firmware/nxt-lcd/src/ferris.rs FERRIS
convert firmware/nxt-boot/assets/wrench.svg firmware/nxt-boot/src/wrench.rs WRENCH
