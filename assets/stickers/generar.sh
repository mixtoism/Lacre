#!/usr/bin/env bash
# Regenera el paquete de stickers por defecto (D-056): SVG → PNG de 256 px
# con resvg, y empaquetado en crates/lacre-node/assets/.
set -euo pipefail
cd "$(dirname "$0")"
python3 -I generar.py
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT
for f in svg/*.svg; do
  resvg -w 256 -h 256 "$f" "$out/$(basename "$f" .svg).png"
done
cargo run -q -p lacre-node --example empaquetar_stickers -- \
  Lacre ../../crates/lacre-node/assets/predeterminados.lacre-stickers "$out"/*.png
