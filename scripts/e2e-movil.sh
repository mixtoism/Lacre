#!/usr/bin/env bash
# Prueba completa entre un móvil Android conectado por adb y este PC, sobre
# Tor: no hace falta que estén en la misma red (el móvil puede ir por datos).
#
# En el PC arranca A y B (con identidad) y C (sin identidad). En el móvil
# ejecuta app/integration_test/movil_test.dart, que:
#   1. crea la identidad «movil»;
#   2. abre un chat 1:1 con A (ping/pong);
#   3. crea un grupo con A y B (ping/pong de los dos);
#   4. recibe un chat que B abre con el código del móvil;
#   5. escribe en el grupo con A apagado; A lo recibe al volver (buzones);
#   6. vincula C como segundo dispositivo; C entra en el grupo y tanto C
#      como el móvil reciben lo que escribe B.
#
# Uso: scripts/e2e-movil.sh [id-del-dispositivo] [directorio-de-trabajo]
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
PATH=$PATH:$HOME/Android/Sdk/platform-tools
DEVICE=${1:-$(adb devices | awk 'NR > 1 && $2 == "device" {print $1; exit}')}
WORK=${2:-$(mktemp -d)}
BIN=$ROOT/target/debug/p2m-node
BASE_PORT=${BASE_PORT:-47200}
TICK=10
[[ -n $DEVICE ]] || { echo "no hay ningún móvil conectado por adb"; exit 1; }

cargo build -q --manifest-path "$ROOT/Cargo.toml" -p p2m-node
rm -rf "$WORK" && mkdir -p "$WORK" && cd "$WORK"
echo "directorio de trabajo: $WORK · móvil: $DEVICE"

PIDS=()
declare -A NODE_PID NODE_PORT
cleanup() { kill "${PIDS[@]}" 2>/dev/null || true; }
trap cleanup EXIT

port=$BASE_PORT
start() { # nombre
  local name=$1
  if [[ ! -p "$name.in" ]]; then
    mkfifo "$name.in"
    # El script mantiene la FIFO abierta (lectura y escritura, no bloquea)
    # para que el nodo no vea EOF y se pueda reiniciar. Con un
    # `tail -f /dev/null` no sirve: sale cuando el nodo cierra la FIFO.
    local fd; exec {fd}<> "$name.in"
    NODE_PORT[$name]=$port; port=$((port + 1))
  fi
  # Con Tor y sin mDNS: el móvil tiene que llegar por los servicios onion.
  "$BIN" --data "$name" --port "${NODE_PORT[$name]}" --tick "$TICK" --no-mdns < "$name.in" >> "$name.out" 2>> "$name.err" &
  PIDS+=($!); NODE_PID[$name]=$!
}
stop() { kill "${NODE_PID[$1]}"; wait "${NODE_PID[$1]}" 2>/dev/null || true; }
say() { echo "$2" > "$1.in"; }
fail() { echo "FALLO: $1"; for f in a b c; do echo "--- $f.out"; tail -n 30 "$f.out"; done; echo "--- móvil"; tail -n 40 movil.log; exit 1; }
wait_in() { # fichero patrón segundos
  local end=$((SECONDS + $3))
  until grep -q -- "$2" "$1"; do
    [[ $SECONDS -lt $end ]] || fail "$1 no mostró «$2»"
    if [[ $1 == movil.log ]] && ! kill -0 "$TEST_PID" 2>/dev/null; then fail "el test del móvil terminó"; fi
    sleep 1
  done
}
wait_for() { wait_in "$1.out" "$2" "${3:-60}"; }
last() { { grep -o -- "$2" "$1.out" || true; } | tail -1; }
ok() { echo "✔ $1"; }

# Contesta «pong <texto> desde <nombre>» a cada «ping <texto>» del móvil.
responder() { # nodo nombre
  tail -n 0 -F "$1.out" 2>/dev/null | while read -r line; do
    if [[ $line =~ ^\[([0-9]+)\]\ movil#[^:]*:\ (ping .*)$ ]]; then
      say "$1" "send ${BASH_REMATCH[1]} pong ${BASH_REMATCH[2]} desde $2"
    fi
  done &
  PIDS+=($!)
}

start a; start b; start c
sleep 2
say a "init pcana"; say b "init pcbea"
wait_for a "identidad creada"; wait_for b "identidad creada"
wait_for c "link "
LINK_C=$(last c 'link [a-z2-7]*' | awk '{print $2}')
qr_of() { # nodo: espera a que su código lleve la dirección onion
  local end=$((SECONDS + 300))
  while :; do
    say "$1" whoami; sleep 5
    local qr; qr=$(last "$1" 'qr: *p2m:[^ ]*/onion3/[^ ]*' | awk '{print $2}')
    [[ -n $qr ]] && { echo "$qr"; return; }
    [[ $SECONDS -lt $end ]] || fail "$1 sin servicio onion"
  done
}
QR_A=$(qr_of a); QR_B=$(qr_of b)
ok "A y B con servicio onion"
responder a A; responder b B

(cd "$ROOT/app" && flutter test integration_test/movil_test.dart -d "$DEVICE" \
  --dart-define=PC_A="$QR_A" --dart-define=PC_B="$QR_B" --dart-define=PC_C_LINK="$LINK_C") \
  > movil.log 2>&1 &
TEST_PID=$!; PIDS+=($TEST_PID)

wait_in movil.log "P2M_E2E identidad" 600
ok "alta en el móvil: $(grep -o 'P2M_E2E identidad .*' movil.log | awk '{print $3}')"
wait_in movil.log "P2M_E2E ok 1:1" 600
ok "chat 1:1 móvil → A → móvil"
wait_in movil.log "P2M_E2E ok grupo" 600
ok "grupo móvil + A + B"

wait_in movil.log "P2M_E2E qr=" 600
QR_M=$(grep -o 'P2M_E2E qr=[^ ]*' movil.log | head -1 | cut -d= -f2-)
say b "chat $QR_M"
wait_for b "chat \[[0-9]*\] creado" 600
say b "send $(last b 'chat \[[0-9]*\] creado' | grep -o '[0-9]*') hola movil desde B"
wait_in movil.log "P2M_E2E ok chat desde B" 600
ok "chat abierto desde el PC con el código del móvil"

wait_in movil.log "P2M_E2E apaga A" 120
stop a
wait_in movil.log "P2M_E2E enciende A" 120
start a
wait_in movil.log "P2M_E2E ok buzones" 900
ok "mensaje a A desconectado entregado al volver"

wait_in movil.log "P2M_E2E vinculado" 300
wait_for c "vinculado a movil#" 300
wait_for c "nuevo chat .*prueba" 600
GROUP_B=$(grep -o '\[[0-9]*\] prueba' b.out | head -1 | tr -dc 0-9)
say b "send $GROUP_B pong vinculado desde B"
wait_for c "pcbea#.*: pong vinculado desde B" 600
wait_in movil.log "P2M_E2E ok vinculación" 600
ok "C vinculado al móvil: entra en el grupo y recibe los mensajes"

wait "$TEST_PID" || fail "el test del móvil falló"
echo "todas las pruebas pasaron"
