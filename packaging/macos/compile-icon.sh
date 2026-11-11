#!/bin/sh
# Builds "DTEW Cord.icns" from the brand artwork into the given Resources directory.
set -eu
resources=$1
root=$(cd "$(dirname "$0")/../.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
set_dir="$work/DTEW Cord.iconset"
mkdir -p "$set_dir" "$resources"
for size in 16 32 128 256 512; do
	sips -z $size $size "$root/assets/brand/dtewcord-1024.png" --out "$set_dir/icon_${size}x${size}.png" >/dev/null
	double=$((size * 2))
	sips -z $double $double "$root/assets/brand/dtewcord-1024.png" --out "$set_dir/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$set_dir" -o "$resources/DTEW Cord.icns"
