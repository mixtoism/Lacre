#!/usr/bin/env bash
# Prueba de extremo a extremo en local: un relay, Alicia (2 dispositivos), Bob
# (1 + 1 vinculado después), Carol, Dan y Eve. Chat 1:1, grupo, vinculación
# tardía, revocación, entrega diferida (buzones), modo de reposo, alta en un chat estando
# desconectado, quitar y volver a añadir a una persona de un grupo, stickers,
# archivos, un nodo sin conexiones entrantes, solicitudes de chat, fotos
# de perfil, búsqueda, bloqueo y borrado de chats (tras NAT) vía relay.
#
# Uso: scripts/e2e.sh [directorio-de-trabajo]
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
WORK=${1:-$(mktemp -d)}
BIN=$ROOT/target/debug/lacre-node
BASE_PORT=${BASE_PORT:-47100}
TICK=6

cargo build -q --manifest-path "$ROOT/Cargo.toml" -p lacre-node
rm -rf "$WORK" && mkdir -p "$WORK" && cd "$WORK"
echo "directorio de trabajo: $WORK"

PIDS=()
declare -A NODE_PID
cleanup() { kill "${PIDS[@]}" 2>/dev/null || true; }
trap cleanup EXIT

port=$BASE_PORT
key_of() { printf 'clave-%s' "$1" | sha256sum | cut -c1-64; }
start() { # nombre [opciones extra]
  local name=$1; shift
  [[ -p "$name.in" ]] || mkfifo "$name.in"
  tail -f /dev/null > "$name.in" & PIDS+=($!)
  local boot=()
  [[ $port != "$BASE_PORT" ]] && boot=(--bootstrap "/ip4/127.0.0.1/tcp/$BASE_PORT")
  # Todos con el estado cifrado (D-033) salvo el relay, que se migra al final.
  local key=; [[ $name != relay ]] && key=$(key_of "$name")
  env ${key:+LACRE_DATA_KEY=$key} "$BIN" --data "$name" --port "$port" --tick "$TICK" --no-rendezvous --no-tor --no-mdns "${boot[@]}" "$@" < "$name.in" >> "$name.out" 2>> "$name.err" &
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
wait_for bob "\(nuevo\|solicitud de\) chat \[0\]"; wait_for alice2 "\(nuevo\|solicitud de\) chat \[0\]"
say bob "send 0 hola alicia"
wait_for alice1 "bob#.*: hola alicia"; wait_for alice2 "bob#.*: hola alicia"
ok "chat 1:1 entregado a los dos dispositivos de Alicia"
say bob "chat $HA"
wait_for bob "chat \[0\] creado"
sleep 2
refute alice1 "\(nuevo\|solicitud de\) chat \[1\]"
ok "un chat 1:1 que ya existe no se duplica"

say carol "group amigos $HA $HB"
wait_for alice1 "\(nuevo\|solicitud de\) chat \[1\] amigos"; wait_for alice2 "\(nuevo\|solicitud de\) chat \[1\] amigos"; wait_for bob "\(nuevo\|solicitud de\) chat \[1\] amigos"
say alice2 "send 1 hola grupo"
wait_for carol "portatil): hola grupo"; wait_for bob "portatil): hola grupo"; wait_for alice1 "portatil): hola grupo"
ok "grupo de 3 personas / 4 dispositivos"

start bob2; sleep 2
say bob "link $(field bob2 'link [a-z2-7]*') movil"
wait_for bob2 "vinculado a $HB"
wait_for bob2 "\(nuevo\|solicitud de\) chat .*alice" $((TICK * 3)); wait_for bob2 "\(nuevo\|solicitud de\) chat .*amigos" $((TICK * 3))
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

# --- Modo de reposo (D-034): sin keepalive ni mDNS y con ticks largos, pero
# se le sigue llegando (aquí por TCP; en el móvil, por su servicio onion).
say carol "power low"
wait_for carol "modo de energía: reposo"
say bob "send 1 en reposo"
wait_for carol "bob#.*: en reposo" 60
say carol "send 0 desde el reposo"
wait_for bob "carol#.*: desde el reposo" 60
say carol "power full"
wait_for carol "modo de energía: normal"
ok "un nodo en reposo recibe y envía"

# --- Quitar y volver a añadir a una persona (Carol creó «amigos»: es su
# chat 0 y el 1 de los demás).
say alice1 "remove 1 $HB"
wait_for alice1 "solo puede añadir o quitar personas el dispositivo que creó"
say carol "remove 0 $HB"
wait_for alice1 "· carol#.* quitó a bob#"; wait_for bob "· carol#.* quitó a bob#"
wait_for bob "expulsado" 30; wait_for bob2 "expulsado" 30
say carol "send 0 sin bob"
wait_for alice1 "carol#.*: sin bob"
sleep 2
refute bob "sin bob"; refute bob2 "sin bob"
ok "persona quitada de un grupo: ya no recibe"
say carol "add 0 $HB"
wait_for bob "· carol#.* añadió a bob#" 60; wait_for alice1 "· carol#.* añadió a bob#"
say carol "send 0 con bob"
wait_for bob "carol#.*: con bob"; wait_for bob2 "carol#.*: con bob"; wait_for alice1 "carol#.*: con bob"
say bob "send 1 he vuelto"
wait_for carol "bob#.*: he vuelto"; wait_for alice1 "bob#.*: he vuelto"
ok "persona añadida de nuevo al grupo con todos sus dispositivos"

# --- Stickers (D-025): imagen dentro del mensaje cifrado, también por buzón.
# Cabecera PNG de 256×256 y relleno aleatorio: el nodo no la decodifica.
mksticker() { printf '\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR\0\0\x01\0\0\0\x01\0' > "$1"; head -c "$2" /dev/urandom >> "$1"; }
mksticker small.png 2000; mksticker big.png 190000; mksticker huge.png 300000
say carol "sticker 0 small.png"
wait_for bob "carol#.*: \[sticker [0-9a-f]*.png\]"; wait_for alice1 "carol#.*: \[sticker [0-9a-f]*.png\]"
SID=$(grep -o "carol#.*: \[sticker [0-9a-f]*.png\]" bob.out | head -1 | grep -o "[0-9a-f]\{64\}.png")
cmp small.png "bob/media/$SID"; cmp small.png "alice1/media/$SID"
say carol "sticker 0 huge.png"
wait_for carol "máximo 200 KB"
stop carol
say bob "sticker 1 big.png"
wait_for bob "guardado en buzones" 60
start carol
wait_for carol "bob#.*: \[sticker [0-9a-f]*.png\]" $((TICK * 4))
BID=$(grep -o "bob#.*: \[sticker [0-9a-f]*.png\]" carol.out | head -1 | grep -o "[0-9a-f]\{64\}.png")
cmp big.png "carol/media/$BID"
ok "stickers entregados (190 KB por buzón; los de más de 200 KB se rechazan)"

# --- Archivos (D-026): oferta cifrada en el mensaje, trozos cifrados que
# sirve cualquier miembro del grupo que ya los tenga.
head -c 3000000 /dev/urandom > doc.bin
say carol "file 0 doc.bin"
wait_for bob "carol#.*: \[archivo doc.bin · 3000000 bytes"; wait_for alice1 "carol#.*: \[archivo doc.bin · 3000000 bytes"
FID=$(grep -o "\[archivo doc.bin · 3000000 bytes · [0-9a-f]*\]" bob.out | head -1 | grep -o "[0-9a-f]\{64\}")
say bob "get $FID"
wait_for bob "archivo descargado" 60
cmp doc.bin "bob/files/$FID/doc.bin"
ok "archivo de 3 MB enviado y descargado"
stop carol
say alice1 "get $FID"
wait_for alice1 "archivo descargado" 60
cmp doc.bin "alice1/files/$FID/doc.bin"
start carol
ok "archivo descargado de otro miembro con el remitente desconectado"
truncate -s 101M enorme.bin
say bob "file 1 enorme.bin"
wait_for bob "máximo 100 MB"
refute carol "enorme.bin"
ok "archivos de más de 100 MB rechazados"

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
wait_for dan "\(nuevo\|solicitud de\) chat" $((TICK * 4)); wait_for dan "alice#.*: hola dan" $((TICK * 4))
ok "añadido a un chat y recibido el mensaje estando desconectado"

# --- Nodo sin conexiones entrantes (como tras NAT): solo vía relay.
start eve --no-listen
wait_for eve "alcanzable a través de relay" 30
say eve "init eve"; wait_for eve "identidad creada"
HE=$(field eve "identidad creada: .*")
sleep 2
say bob "chat $HE"
wait_for eve "\(nuevo\|solicitud de\) chat \[0\]" 60
say eve "send 0 hola desde detras del nat"
wait_for bob "eve#.*: hola desde detras del nat"
ok "nodo tras NAT alcanzable a través del relay"

# --- Chats 1:1 duplicados (D-027): Carol y Dan se abren chat a la vez; cada
# uno ve uno solo, con los mensajes de los dos grupos MLS.
say carol "chat $HD"; say dan "chat $HC"
wait_for carol "chat \[1\] creado" 90; wait_for dan "chat \[1\] creado" 90
sleep $((TICK + 2))
say carol "send 1 hola dan"; say dan "send 1 hola carol"
wait_for dan "\[1\] carol#.*: hola dan"; wait_for carol "\[1\] dan#.*: hola carol"
for n in carol dan; do
  : > $n.out; say $n chats; wait_for $n "\[1\]"; sleep 1
  [[ $(grep -c "\] \(carol\|dan\)#" $n.out) == 1 ]] || { echo "FALLO: $n ve el chat 1:1 duplicado"; cat $n.out; exit 1; }
done
ok "chats 1:1 creados a la vez: cada uno ve uno solo"

# --- Solicitudes de chat, fotos de perfil, búsqueda, bloqueo y borrado
# (D-029, D-030). Fran no conoce a nadie: su chat llega como solicitud.
start fran; sleep 2
say fran "init fran"; wait_for fran "identidad creada"
HF=$(field fran "identidad creada: .*")
mksticker cara.png 3000
say fran "avatar cara.png"; wait_for fran "foto de perfil actualizada"
say fran "chat $HB"; wait_for fran "chat \[0\] creado" 60
wait_for bob "solicitud de chat \[[0-9]*\] $HF" 60; wait_for bob2 "solicitud de chat \[[0-9]*\] $HF" 60
FA=$(grep -o "solicitud de chat \[[0-9]*\] $HF" bob.out | head -1 | grep -o "\[[0-9]*\]" | tr -d '[]')
say fran "send 0 Hola, ¿qué tal? ZANAHÓRIA"; wait_for bob "fran#.*: Hola, ¿qué tal?"
say fran "file 0 doc.bin"; wait_for bob "fran#.*: \[archivo doc.bin"
FF=$(grep -o "fran#.*: \[archivo doc.bin · [0-9]* bytes · [0-9a-f]*\]" bob.out | head -1 | grep -o "[0-9a-f]*\]$" | tr -d ']')
say bob "get $FF"; wait_for bob "acepta antes la solicitud"
refute bob "foto de perfil de $HF"
: > bob.out; say bob chats; wait_for bob "\[$FA\] $HF.*(solicitud)"
ok "chat de un desconocido: llega como solicitud, sin su foto y sin poder descargar"
: > bob.out; : > bob2.out
say bob "accept $FA"
wait_for bob "foto de perfil de $HF" 30
cmp cara.png bob/avatars/"${HF#*#}"-*.png
say bob "get $FF"; wait_for bob "archivo descargado" 60
cmp doc.bin "bob/files/$FF/doc.bin"
sleep 3; say bob2 chats; wait_for bob2 "\] $HF"; sleep 1; refute bob2 "(solicitud)"
ok "solicitud aceptada: llegan su foto y el archivo; el portátil también la da por aceptada"

mksticker cara2.png 4000
say bob "avatar cara2.png"; wait_for fran "foto de perfil de $HB" 30
cmp cara2.png fran/avatars/"${HB#*#}"-*.png
ok "foto de perfil propia anunciada y descargada por el otro"

: > bob.out; say bob "search zanahoria"; wait_for bob "resultado(s)"
grep -q "\[$FA#[0-9]*\] fran#.*ZANAHÓRIA" bob.out || { echo "FALLO: búsqueda global"; cat bob.out; exit 1; }
: > bob.out; say bob "search $FA:QUÉ TAL"; wait_for bob "^1 resultado(s)"
: > bob.out; say bob "search 0:zanahoria"; wait_for bob "^0 resultado(s)"
ok "búsqueda global y dentro de un chat, sin distinguir mayúsculas ni tildes"

say bob "block $HF"; wait_for bob "bloqueado"
sleep 3; : > bob2.out; say bob2 blocked; wait_for bob2 "  $HF"
: > bob.out
say fran "send 0 sigues ahi"; say fran "group pruebas $HB"; wait_for fran "grupo \[1\] creado" 60
sleep 4; refute bob "sigues ahi"; refute bob "pruebas"
say bob chats; sleep 2; refute bob "$HF"
say bob "unblock $HF"; wait_for bob "desbloqueado"
sleep 3; : > bob2.out; say bob2 blocked; sleep 1; refute bob2 "$HF"
: > bob.out; say bob chats; wait_for bob "\[$FA\] $HF"; wait_for bob "pruebas.*(solicitud)"
ok "bloqueo: sus mensajes e invitaciones se ignoran; al desbloquear vuelven sus chats"
ok "bloqueo y desbloqueo sincronizados con el portátil"

# Borrar un grupo es salir de él (D-031): se lo pide al creador, que nos
# quita; si nos vuelve a añadir, llega otra vez como solicitud.
GP=$(grep -o "\[[0-9]*\] pruebas" bob.out | head -1 | grep -o "[0-9][0-9]*")
: > fran.out; : > bob.out; : > bob2.out
say bob "delete $GP"; wait_for bob "borrado"
for i in $(seq 30); do say fran "history 1"; sleep 2; grep -q "$HB salió del chat" fran.out && break; done
wait_for fran "$HB salió del chat" 1
wait_for bob2 "Saliste del chat desde otro dispositivo" 30
sleep 3; say fran "send 1 ya no estas"; sleep 4
refute bob "ya no estas"; refute bob2 "ya no estas"; refute bob "expulsado"
: > bob.out; say bob chats; sleep 2; refute bob "pruebas"
say fran "add 1 $HB"; wait_for bob "solicitud de chat \[$GP\]" 60
ok "borrar un grupo es salir de él; si nos vuelven a añadir, es una solicitud"

say bob "delete $FA"; wait_for bob "borrado"
: > bob.out; say bob chats; sleep 2; refute bob "\[$FA\]"
say fran "send 0 vuelvo"; wait_for bob "fran#.*: vuelvo"
: > bob.out; say bob "history $FA"; wait_for bob "vuelvo"; sleep 1; refute bob "ZANAH"
ok "chat borrado: desaparece y vuelve vacío con el siguiente mensaje"

# Cifrado en disco (D-033): nada legible sin la clave, el relay (en claro)
# se migra al arrancar con clave, y sin ella ya no arranca.
for f in device.key history.cbor chats.cbor mls.json; do
  head -c 8 "alice1/$f" | grep -q P2MVLT1 || { echo "FALLO: alice1/$f no está cifrado"; exit 1; }
done
if grep -rqa "hola dan" alice1 dan; then echo "FALLO: texto de un mensaje en claro en disco"; exit 1; fi
[[ $(stat -c %s relay/device.key) == 32 ]] || { echo "FALLO: el relay ya estaba cifrado"; exit 1; }
stop relay
RELAYKEY=$(key_of relay)
LACRE_DATA_KEY=$RELAYKEY "$BIN" --data relay --headless --no-tor --no-mdns --no-rendezvous --port 0 > relay2.out 2>&1 & RP=$!; PIDS+=($RP)
for _ in $(seq 40); do grep -q "dispositivo" relay2.out && break; sleep 0.25; done
kill $RP; wait $RP 2>/dev/null || true
head -c 8 relay/device.key | grep -q P2MVLT1 || { echo "FALLO: el relay no se migró"; cat relay2.out; exit 1; }
if "$BIN" --data relay --headless --no-tor --no-mdns --no-rendezvous --port 0 > relay3.out 2>&1; then
  echo "FALLO: arrancó sin la clave de datos"; exit 1
fi
grep -q "falta la clave de datos" relay3.out || { echo "FALLO: error inesperado"; cat relay3.out; exit 1; }
LACRE_DATA_KEY=$(key_of otra) "$BIN" --data relay --headless --no-tor --no-mdns --no-rendezvous --port 0 > relay4.out 2>&1 && { echo "FALLO: arrancó con otra clave"; exit 1; }
grep -q "no se pudo descifrar" relay4.out || { echo "FALLO: error inesperado con otra clave"; cat relay4.out; exit 1; }
ok "estado cifrado en disco; un nodo en claro se migra; sin la clave (o con otra) no arranca"

echo "todas las pruebas pasaron"
