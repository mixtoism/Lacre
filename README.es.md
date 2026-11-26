<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/brand/lacre-logo-dark.svg">
    <img src="docs/brand/lacre-logo.svg" alt="Lacre" width="320">
  </picture>
</p>

# Lacre

Mensajería P2P con cifrado de extremo a extremo, multidispositivo y sin
servidores intermedios.

> **Estado: endurecida, sin auditar.** Se ha aplicado una revisión de
> seguridad interna (D-039 a D-057), pero el protocolo (`v1`) aún puede
> cambiar y el código no ha pasado ninguna auditoría independiente. No lo
> uses todavía para nada delicado.

La versión completa, con el protocolo y la referencia de la CLI, está en
inglés en [`README.md`](README.md). Las decisiones de diseño, con sus motivos
y lo que queda pendiente, están en [`docs/DECISIONES.md`](docs/DECISIONES.md);
el protocolo, en [`docs/PROTOCOLO.md`](docs/PROTOCOLO.md), y lo que falta, en
[`docs/HOJA_DE_RUTA.md`](docs/HOJA_DE_RUTA.md).

## Qué hace

- **Identidad:** una clave raíz Ed25519 por persona. Handle verificable
  `nombre#sufijo`, con el sufijo derivado de la clave (Argon2id), o QR. Frase
  de recuperación de 24 palabras (D-048).
- **Multidispositivo:** la raíz firma la lista de dispositivos. Vincular uno
  nuevo pide el secreto de un solo uso de su código y un código de 6 cifras
  que se compara en las dos pantallas (D-039, D-047). Los nuevos entran solos
  en los chats y reciben el historial anterior de otro dispositivo propio
  (D-050); los revocados quedan fuera.
- **Cifrado:** MLS (RFC 9420). Cada dispositivo es un miembro; los chats 1:1 y
  los grupos usan el mismo mecanismo. Número de seguridad para comprobar un
  contacto (D-043). Quien gestiona un grupo puede traspasarlo (D-049).
- **Descubrimiento:** DHT Kademlia propia (libp2p) y mDNS en la wifi.
- **Entre redes, sin servidores propios:** cada nodo publica un servicio onion
  de Tor, alcanzable tras NAT o CGNAT, y comprueba desde fuera que sigue
  publicado (D-053). El primer contacto llega por QR y después por una caché
  de peers. Los nodos con IP pública pueden ofrecer relay (con AutoNAT, D-051)
  y anunciarse en la DHT Mainline de BitTorrent y en trackers. IPv6 (D-052).
- **Mensajes a dispositivos desconectados:** se dejan sellados en buzones de
  otros nodos, que no pueden leerlos.
- **Archivos:** hasta 100 MB, en trozos cifrados que sirve cualquier miembro.
  Hasta 32 MiB se depositan además 7 días en dos nodos de la DHT, para
  descargarlos con el remitente desconectado (D-054).
- **Stickers:** PNG, GIF, WebP o JPEG de hasta 200 KB. Un GIF animado que no
  cabe se convierte a WebP animado. La colección se comparte como un archivo
  `.lacre-stickers` (D-055). Se empieza con 13 stickers propios de un sello
  de lacre (D-056).
- **Cifrado en disco:** estado, historial, fotos y archivos, con una clave del
  almacén del sistema (Keystore de Android, Secret Service, Llavero de macOS,
  DPAPI). Se detecta la vuelta atrás de un archivo de estado (D-046).
- **Nodo siempre encendido:** `--headless` para systemd o Docker: hace de
  buzón, de depósito y, con IP pública y `--relay-server`, de relay.

## Estructura

```
crates/lacre-core   identidad, registros firmados, MLS, sobres (sin red)
crates/lacre-node   red libp2p, lógica de chats, CLI
app/                app Flutter (Linux y Android); app/rust es el puente lacre_api
scripts/e2e.sh      prueba de extremo a extremo con varios nodos locales
deploy/             servicio systemd y Dockerfile para un nodo siempre encendido
fuzz/               fuzzing de los decodificadores
```

## Probarlo

```sh
cargo test                 # pruebas unitarias
scripts/e2e.sh             # escenario completo en local (sin Tor)
```

A mano, en dos terminales (o dos máquinas de la misma red local):

```sh
cargo run -p lacre-node -- --data /tmp/ana --port 4001
> init ana
> whoami                       # muestra ana#xxxxxxxx y el QR

cargo run -p lacre-node -- --data /tmp/beto --port 4002 --bootstrap /ip4/127.0.0.1/tcp/4001
> init beto
> chat ana#xxxxxxxx
> send 0 hola
```

`help` lista todos los comandos: grupos, archivos, stickers, bloqueo,
búsqueda, recuperación, vinculación…

Para vincular un segundo dispositivo, arráncalo sin `init`: mostrará un código.
Después ejecuta `link <código> portatil` en el dispositivo primario y compara
el código de confirmación que muestran los dos.

### Entre redes distintas

Tor está activado por defecto:

```sh
# máquina A
cargo run -p lacre-node -- --data /tmp/ana
> init ana
> whoami      # espera a «servicio onion publicado»; el QR incluye /onion3/…

# máquina B, en cualquier otra red
cargo run -p lacre-node -- --data /tmp/beto
> init beto
> chat p2m:ana#…:…@/onion3/…     # pega el QR completo de A
> send 0 hola
```

- Publicar el servicio onion suele llevar menos de un minuto; la primera vez
  a veces tarda varios. Cada conexión por onion tarda unos segundos.
- Los peers vistos se guardan en `<datos>/peers.txt`; al reiniciar se
  reconecta solo.
- `--no-tor` desactiva Tor; `--bootstrap <multiaddr>` da un nodo de entrada
  (vale una `/onion3/…`); `addrs` muestra las direcciones propias.

## App

Requiere Flutter y Rust; la parte en Rust se compila sola con Flutter.

```sh
cd app
flutter run -d linux                              # escritorio (GTK 3)
LACRE_DATA=/tmp/lacre-b flutter run -d linux      # segunda instancia independiente
flutter build apk                                 # Android (SDK, NDK y JDK 17)
```

Para Android hace falta un JDK 17 completo, no un JRE. Si el `java` del
`PATH` es un JRE, Gradle falla con «does not provide the required
capabilities: [JAVA_COMPILER]»: `flutter config --jdk-dir=/usr/lib/jvm/<jdk-17>`.

Tras cambiar `app/rust/src/api`, se regeneran los enlaces con
`flutter_rust_bridge_codegen generate` (desde `app/`).

## Limitaciones actuales

- Sin auditoría externa: no se puede afirmar que sea segura.
- Tor no es un modo anónimo: también se anuncian las direcciones IP.
- Sin iOS, Windows ni macOS todavía, y sin notificaciones push. En Linux,
  al cerrar la ventana el nodo sigue en marcha con un icono en la bandeja
  si el escritorio la tiene (D-057); en GNOME hace falta la extensión
  AppIndicator.
- Solo un dispositivo por grupo hace cambios de miembros; si está
  desconectado, esperan.
- La clave raíz no se puede rotar: el handle se deriva de ella.
- Más detalle en [`docs/DECISIONES.md`](docs/DECISIONES.md) y en la sección
  «Limitations and roadmap» de [`README.md`](README.md).

## Licencia

[GPL-3.0 o posterior](LICENSE).
