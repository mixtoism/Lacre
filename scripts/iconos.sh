#!/usr/bin/env bash
# Regenera los iconos de la app a partir de los SVG de docs/brand/.
# Requiere ImageMagick 7 (`magick`) con soporte librsvg.
set -euo pipefail

raiz="$(cd "$(dirname "$0")/.." && pwd)"
brand="$raiz/docs/brand"
res="$raiz/app/android/app/src/main/res"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

svg() { # svg <fichero> <lado> <salida>
  magick -background none -density 600 "$1" -resize "$2x$2" -depth 8 "$3"
}

# Android: icono heredado (48dp) y capas del icono adaptativo (108dp, con el
# sello dentro de la zona segura de 66dp).
declare -A dpi=([mdpi]=1 [hdpi]=1.5 [xhdpi]=2 [xxhdpi]=3 [xxxhdpi]=4)
for d in "${!dpi[@]}"; do
  f=${dpi[$d]}
  lado=$(awk "BEGIN{print int(48*$f)}")
  capa=$(awk "BEGIN{print int(108*$f)}")
  sello=$(awk "BEGIN{print int(108*$f*0.75)}")
  dir="$res/mipmap-$d"
  mkdir -p "$dir"
  svg "$brand/lacre-app-icon.svg" "$lado" "$dir/ic_launcher.png"
  svg "$brand/lacre-icon.svg" "$sello" "$tmp/fg.png"
  magick "$tmp/fg.png" -background none -gravity center -extent "${capa}x${capa}" -depth 8 \
    "$dir/ic_launcher_foreground.png"
  # Monocromo (iconos temáticos de Android 13): sólo cuenta el alfa, así que se
  # quita el recorte crema de la versión plana.
  svg "$brand/lacre-icon-flat.svg" "$sello" "$tmp/flat.png"
  magick "$tmp/flat.png" -fuzz 25% -transparent "#FBF4EC" -fill white -colorize 100 \
    -background none -gravity center -extent "${capa}x${capa}" -depth 8 \
    "$dir/ic_launcher_monochrome.png"
done

# Linux: icono de ventana.
mkdir -p "$raiz/app/linux/runner/resources"
svg "$brand/lacre-app-icon.svg" 256 "$raiz/app/linux/runner/resources/app_icon.png"

# Optimización opcional.
if command -v oxipng >/dev/null; then
  oxipng -q -o 4 --strip safe "$res"/mipmap-*/*.png "$raiz/app/linux/runner/resources/app_icon.png"
fi

# Windows: .ico con varios tamaños.
for l in 16 24 32 48 64 128 256; do
  svg "$brand/lacre-app-icon.svg" "$l" "$tmp/win-$l.png"
done
magick "$tmp"/win-{16,24,32,48,64,128,256}.png "$raiz/app/windows/runner/resources/app_icon.ico"

# macOS: juego de AppIcon.
for l in 16 32 64 128 256 512 1024; do
  svg "$brand/lacre-app-icon.svg" "$l" \
    "$raiz/app/macos/Runner/Assets.xcassets/AppIcon.appiconset/app_icon_$l.png"
done
