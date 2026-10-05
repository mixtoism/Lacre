#!/usr/bin/env bash
# Prueba de extremo a extremo en local: un relay, Alicia (2 dispositivos), Bob
# (1 + 1 vinculado después), Carol, Dan y Eve. Chat 1:1, grupo, vinculación
# tardía, revocación, entrega diferida (buzones), alta en un chat estando
# desconectado y un nodo sin conexiones entrantes (tras NAT) vía relay.
#
# Uso: scripts/e2e.sh [directorio-de-trabajo]
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
WORK=${1:-$(mktemp -d)}
BIN=$ROOT/target/debug/p2m-node
BASE_PORT=${BASE_PORT:-47100}
TICK=6

cargo build -q --manifest-path "$ROOT/Cargo.toml" -p p2m-node
rm -rf "$WORK" && mkdir -p "$WORK" && cd "$WORK"
echo "directorio de trabajo: $WORK"

PIDS=()
declare -A NODE_PID
cleanup() { kill "${PIDS[@]}" 2>/dev/null || true; }
trap cleanup EXIT

port=$BASE_PORT
start() { # nombre [opciones extra]
  local name=$1; shift
  [[ -p "$name.in" ]] || mkfifo "$name.in"
  tail -f /dev/null > "$name.in" & PIDS+=($!)
  local boot=()
  [[ $port != "$BASE_PORT" ]] && boot=(--bootstrap "/ip4/127.0.0.1/tcp/$BASE_PORT")
  "$BIN" --data "$name" --port "$port" --tick "$TICK" --no-rendezvous --no-tor "${boot[@]}" "$@" < "$name.in" >> "$name.out" 2>> "$name.err" &
  PIDS+=($!); NODE_PID[$name]=$!
  port=$((port + 1))
}
stop() { kill "${NODE_PID[$1]}"; wait "${NODE_PID[$1]}" 2>/dev/null || true; }
say() { echo "$2" > "$1.in"; }
field() { grep -o "$2" "$1.out" | head -1 | awk '{print $NF}'; }
wait_for() { # nodo patrón [segundos]
  for _ in $(seq $((${3:-20} * 4))); do
    grep -q -- "$2" "$1.out" && return 0
    sleep 0.25
  done
  echo "FALLO: $1 no mostró «$2»"; echo "--- $1.out"; cat "$1.out"; exit 1
}
refute() { # nodo patrón
  if grep -q -- "$2" "$1.out"; then echo "FALLO: $1 mostró «$2»"; exit 1; fi
}
ok() { echo "✔ $1"; }

start relay --relay-server; sleep 1
start alice1; start alice2; start bob; start carol
sleep 2
for n in alice1 bob carol; do say $n "init ${n%1}"; done
for n in alice1 bob carol; do wait_for $n "identidad creada"; done
HA=$(field alice1 "identidad creada: .*"); HB=$(field bob "identidad creada: .*"); HC=$(field carol "identidad creada: .*")
ok "identidades: $HA $HB $HC"

say alice1 "link $(field alice2 'link [a-z2-7]*') portatil"
wait_for alice2 "vinculado a $HA"
ok "portátil de Alicia vinculado"

say alice1 "chat $HB"
wait_for bob "nuevo chat \[0\]"; wait_for alice2 "nuevo chat \[0\]"
say bob "send 0 hola alicia"
wait_for alice1 "bob#.*: hola alicia"; wait_for alice2 "bob#.*: hola alicia"
ok "chat 1:1 entregado a los dos dispositivos de Alicia"

say carol "group amigos $HA $HB"
wait_for alice1 "nuevo chat \[1\] amigos"; wait_for alice2 "nuevo chat \[1\] amigos"; wait_for bob "nuevo chat \[1\] amigos"
say alice2 "send 1 hola grupo"
wait_for carol "portatil): hola grupo"; wait_for bob "portatil): hola grupo"; wait_for alice1 "portatil): hola grupo"
ok "grupo de 3 personas / 4 dispositivos"

start bob2; sleep 2
say bob "link $(field bob2 'link [a-z2-7]*') movil"
wait_for bob2 "vinculado a $HB"
wait_for bob2 "nuevo chat .*alice" $((TICK * 3)); wait_for bob2 "nuevo chat .*amigos" $((TICK * 3))
say bob2 "send 0 desde el movil"
wait_for alice2 "(movil): desde el movil"
refute alice2 "no autorizado"
ok "dispositivo vinculado tarde entra en los chats existentes"

say alice1 "unlink $(field alice2 'link [a-z2-7]*')"
wait_for alice2 "expulsado" $((TICK * 3))
sleep $((TICK * 2))
say carol "send 0 tras revocar"
wait_for bob2 "carol#.*: tras revocar"; wait_for alice1 "carol#.*: tras revocar"
sleep 2
refute alice2 "tras revocar"
ok "dispositivo revocado expulsado y sin acceso a mensajes nuevos"

# --- Entrega diferida: Carol se desconecta y recibe al volver.
stop carol
say bob "send 1 mientras dormias"
wait_for bob "guardado en buzones" 60
start carol
wait_for carol "bob#.*: mientras dormias" $((TICK * 4))
ok "mensaje a un dispositivo desconectado entregado al volver (buzones)"

# --- Alta en un chat estando desconectado (KeyPackage de último recurso).
start dan; sleep 2
say dan "init dan"; wait_for dan "identidad creada"
HD=$(field dan "identidad creada: .*")
sleep $((TICK + 2))   # publica registro y KeyPackage en la DHT
stop dan
say alice1 "chat $HD"
wait_for alice1 "chat \[2\] creado" 90
say alice1 "send 2 hola dan"
wait_for alice1 "guardado en buzones" 60
start dan
wait_for dan "nuevo chat" $((TICK * 4)); wait_for dan "alice#.*: hola dan" $((TICK * 4))
ok "añadido a un chat y recibido el mensaje estando desconectado"

# --- Nodo sin conexiones entrantes (como tras NAT): solo vía relay.
start eve --no-listen
wait_for eve "alcanzable a través de relay" 30
say eve "init eve"; wait_for eve "identidad creada"
HE=$(field eve "identidad creada: .*")
sleep 2
say bob "chat $HE"
wait_for eve "nuevo chat \[0\]" 60
say eve "send 0 hola desde detras del nat"
wait_for bob "eve#.*: hola desde detras del nat"
ok "nodo tras NAT alcanzable a través del relay"

echo "todas las pruebas pasaron"
