# p2message

Mensajería P2P con cifrado de extremo a extremo, multidispositivo y sin
servidores intermedios. Estado: **prueba de concepto** (CLI en Linux).

- **Identidad:** una clave raíz por persona, guardada en el dispositivo.
  Handle verificable `nombre#sufijo`, con el sufijo derivado de la clave, o QR.
- **Descubrimiento:** DHT Kademlia propia (libp2p) y mDNS en la red local.
- **Entre redes, sin servidores propios:** cada nodo publica un servicio onion
  de Tor (alcanzable tras NAT o CGNAT). El primer contacto llega por QR, que
  lleva esa dirección, y después por una caché de peers. Los nodos con IP
  pública pueden además ofrecer relay y anunciarse en la DHT Mainline de
  BitTorrent y en trackers.
- **Mensajes a dispositivos desconectados:** se dejan sellados en buzones de
  otros nodos.
- **Cifrado:** MLS (RFC 9420). Cada dispositivo es un miembro; los chats 1:1 y
  los grupos usan el mismo mecanismo.
- **Multidispositivo:** la raíz firma la lista de dispositivos. Los dispositivos
  nuevos entran solos en los chats existentes y los revocados quedan fuera.

Las decisiones de diseño y sus motivos están en
[`docs/DECISIONES.md`](docs/DECISIONES.md).

## Estructura

```
crates/p2m-core   identidad, registros firmados, MLS (sin red; reutilizable vía FFI)
crates/p2m-node   red libp2p + lógica de chats + CLI
app/              app Flutter (de momento Linux); app/rust es el puente p2m_api
scripts/e2e.sh    prueba de extremo a extremo con varios nodos locales
deploy/           servicio systemd y Dockerfile para un nodo siempre encendido
```

## Probarlo

```sh
cargo test -p p2m-core     # pruebas unitarias del núcleo
scripts/e2e.sh             # escenario completo en local
```

A mano, en dos terminales (o dos máquinas de la misma red local):

```sh
cargo run -p p2m-node -- --data /tmp/ana --port 4001
> init ana
> whoami                       # muestra ana#xxxxxxxx y el QR

cargo run -p p2m-node -- --data /tmp/beto --port 4002 --bootstrap /ip4/127.0.0.1/tcp/4001
> init beto
> chat ana#xxxxxxxx
> send 0 hola
```

Para vincular un segundo dispositivo, arráncalo sin `init`: mostrará un código.
Después ejecuta `link <código> portatil` en el dispositivo primario.

### Entre redes distintas

mDNS solo funciona en la red local. Entre redes (por ejemplo, casa y móvil
compartiendo datos), Tor está activado por defecto:

```sh
# máquina A
cargo run -p p2m-node -- --data /tmp/ana
> init ana
> whoami      # espera a ver «servicio onion publicado»; el QR incluye /onion3/…

# máquina B, en cualquier otra red
cargo run -p p2m-node -- --data /tmp/beto
> init beto
> chat p2m:ana#…:…@/onion3/…     # pega el QR completo de A
> send 0 hola
```

- Publicar el servicio onion suele llevar menos de un minuto. La primera vez
  (Tor descarga su directorio) a veces tarda varios.
- Cada conexión por onion tarda unos segundos en abrirse.
- Los peers vistos se guardan en `<datos>/peers.txt`; al reiniciar se
  reconecta solo.
- Opciones:
  - `--no-tor` desactiva Tor.
  - `--bootstrap <multiaddr>` da un nodo de entrada (vale una `/onion3/…`).
  - `--no-rendezvous` desactiva la búsqueda de relays en Mainline y trackers.
    La lista de trackers se puede cambiar con `--tracker` o con
    `<datos>/trackers.txt`.
  - `addrs` muestra las direcciones propias.

### Nodo siempre encendido

`--headless` arranca sin consola, para systemd o Docker (ver `deploy/`).
Hace de buzón para los contactos desconectados y, con IP pública y
`--relay-server`, de relay.

## App de escritorio (Linux)

Requiere Flutter, las librerías de desarrollo de GTK 3 y Rust. La parte en
Rust se compila sola con Flutter.

```sh
cd app
flutter run -d linux
P2M_DATA=/tmp/p2m-b flutter run -d linux     # segunda instancia independiente
```

## Limitaciones actuales

- Tor no es un modo anónimo: también se anuncian las direcciones IP.
- Historial en un único archivo CBOR, sin cifrar en disco.
- Interfaz gráfica solo en Linux por ahora (falta Android, iOS, Windows y macOS).
- Más detalle en `docs/DECISIONES.md`.
