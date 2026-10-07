#!/usr/bin/env bash
# Prueba visible de un grupo: varias ventanas de la app de escritorio en este
# PC (una por usuario) y la app en un móvil Android conectado por adb. Se ve
# la interfaz mientras ocurre. Ver app/integration_test/grupo_test.dart.
#
#   1. Cada ventana del PC crea su usuario y espera a tener servicio onion.
#   2. El móvil crea el suyo y abre el grupo «prueba» con los códigos del PC.
#   3. Todos escriben «hola desde <usuario>» y esperan los mensajes de todos.
#   4. El móvil quita al último usuario del PC (que deja de recibir) y lo
#      vuelve a añadir.
#   5. Emojis y stickers: el móvil manda un emoji y su sticker, los PC lo
#      guardan en su colección y mandan el suyo.
#   6. Archivos: el móvil manda uno de 1 MB y cada PC lo descarga.
#   7. Fotos de perfil, búsqueda en el chat y global, y borrar el chat.
#
# Los PC reciben el grupo como solicitud de chat y la aceptan.
#
# Ojo: deja compilada (PC) e instalada (móvil) la versión de prueba; después
# hay que volver a compilar y a instalar la normal.
#
# Por defecto, solo red local (mDNS, sin Tor): el móvil tiene que estar en la
# misma wifi que el PC. Con TOR=1, por Tor (mucho más lento: cada servicio
# onion tarda minutos en estar disponible).
#
# Con `linux` como dispositivo, el papel del móvil lo hace otra ventana del
# PC (sin escáner de QR ni teclado del sistema, pero el mismo recorrido).
#
# Con Hyprland, cada ventana va a su propio escritorio a partir de WS (por
# defecto 5): en mosaico, varias en el mismo quedarían demasiado pequeñas.
#
# Uso: scripts/e2e-grupo.sh [id-del-dispositivo|linux] [directorio-de-trabajo] [usuarios-del-pc…]
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
PATH=$PATH:$HOME/Android/Sdk/platform-tools
DEVICE=${1:-$(adb devices | awk 'NR > 1 && $2 == "device" {print $1; exit}')}
WORK=${2:-$(mktemp -d)}
PCS=(ana bea carl)
if [[ $# -gt 2 ]]; then shift 2; PCS=("$@"); fi
USERS="movil ${PCS[*]}"
[[ -n $DEVICE ]] || { echo "no hay ningún móvil conectado por adb"; exit 1; }
if [[ ${TOR:-0} == 1 ]]; then LAN=0; RED=Tor; else LAN=1; RED="red local"; fi

rm -rf "$WORK" && mkdir -p "$WORK"
echo "directorio de trabajo: $WORK · móvil: $DEVICE · PC: ${PCS[*]} · $RED"
cd "$ROOT/app"
flutter build linux --debug -t integration_test/grupo_test.dart > "$WORK/build.log" 2>&1 \
  || { tail -n 30 "$WORK/build.log"; exit 1; }
BIN=$ROOT/app/build/linux/x64/debug/bundle/p2message

PIDS=()
cleanup() { kill "${PIDS[@]}" 2>/dev/null || true; }
trap cleanup EXIT
fail() { echo "FALLO: $1"; for u in "${PCS[@]}"; do echo "--- $u"; grep P2M_E2E "$WORK/$u.log" | tail -n 5; done
  echo "--- móvil"; tail -n 30 "$WORK/movil.log"; exit 1; }
wait_file() { # fichero segundos
  local end=$((SECONDS + $2))
  until [[ -e $1 ]]; do [[ $SECONDS -lt $end ]] || fail "no apareció $1"; sleep 2; done
}

# Lleva la ventana del proceso a un escritorio (solo con Hyprland).
place() { # pid escritorio
  command -v hyprctl > /dev/null && [[ -n ${HYPRLAND_INSTANCE_SIGNATURE:-} ]] || return 0
  for _ in $(seq 40); do
    if hyprctl clients | grep -q "pid: $1\$"; then
      # Sintaxis Lua (Hyprland ≥ 0.55); si falla, la antigua.
      hyprctl dispatch "hl.dsp.window.move({ workspace = $2, window = \"pid:$1\", follow = false })" > /dev/null 2>&1 \
        || hyprctl dispatch movetoworkspacesilent "$2,pid:$1" > /dev/null 2>&1 || true
      return 0
    fi
    sleep 0.25
  done
}
ws=${WS:-5}
# Clave de datos por usuario (D-033): así las pruebas no tocan el llavero.
key_of() { printf 'clave-%s' "$1" | sha256sum | cut -c1-64; }
for u in "${PCS[@]}"; do
  P2M_DATA_KEY=$(key_of $u) P2M_LAN=$LAN P2M_ROLE=$u P2M_E2E_DIR=$WORK P2M_USERS=$USERS "$BIN" > "$WORK/$u.log" 2>&1 &
  PIDS+=($!)
  place $! $ws; echo "  $u en el escritorio $ws"; ws=$((ws + 1))
done
for u in "${PCS[@]}"; do wait_file "$WORK/$u.qr" 600; done
echo "✔ usuarios del PC listos ($RED)"

QRS=$(for u in "${PCS[@]}"; do cat "$WORK/$u.qr"; echo -n ' '; done)
if [[ $DEVICE == linux ]]; then
  P2M_DATA_KEY=$(key_of movil) P2M_LAN=$LAN P2M_ROLE=movil P2M_PCS="${QRS% }" P2M_E2E_DIR=$WORK P2M_USERS=$USERS "$BIN" > "$WORK/movil.log" 2>&1 &
  place $! $ws
else
  flutter test integration_test/grupo_test.dart -d "$DEVICE" \
    --dart-define=LAN=$([[ $LAN == 1 ]] && echo true || echo false) --dart-define=ROLE=movil --dart-define=PCS="${QRS% }" --dart-define=USERS="$USERS" \
    > "$WORK/movil.log" 2>&1 &
fi
TEST_PID=$!; PIDS+=($TEST_PID)

end=$((SECONDS + 3600))
seen=""
while :; do
  for f in "$WORK"/*.log; do
    while read -r line; do
      [[ $seen == *"|$line|"* ]] && continue
      seen+="|$line|"; echo "  $line"
    done < <(grep -o 'P2M_E2E .*' "$f" || true)
  done
  done_all=1
  for u in "${PCS[@]}"; do [[ -e $WORK/$u.ok ]] || done_all=0; done
  grep -q 'P2M_E2E movil ok' "$WORK/movil.log" || done_all=0
  [[ $done_all == 1 ]] && break
  kill -0 "$TEST_PID" 2>/dev/null || grep -q 'P2M_E2E movil ok' "$WORK/movil.log" || fail "el test del móvil terminó"
  [[ $SECONDS -lt $end ]] || fail "tiempo agotado"
  sleep 2
done
# Al final cada uno borra el chat, cuando todos han terminado.
end=$((SECONDS + 180))
for u in "${PCS[@]}" movil; do
  until grep -q "P2M_E2E $u chat borrado" "$WORK/$u.log"; do
    [[ $SECONDS -lt $end ]] || fail "$u no borró el chat"; sleep 2
  done
  echo "  P2M_E2E $u chat borrado"
done
echo "✔ mensajes de todos a todos; persona quitada y añadida de nuevo; stickers; archivos;"
echo "  solicitudes de chat; fotos de perfil; búsqueda en el chat y global; borrar el chat"
if [[ $DEVICE == linux ]]; then
  # La ventana no termina sola: basta con que haya llegado al final.
  grep -q 'P2M_E2E movil ok' "$WORK/movil.log" || fail "el test del móvil falló"
else
  wait "$TEST_PID" || fail "el test del móvil falló"
fi
echo "todas las pruebas pasaron"
