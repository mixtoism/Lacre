```
Proyecto p2message                                           P. Arnau
Borrador: draft-lacre-protocolo-00                     7 de octubre de 2026
Categoría: Informativo / Experimental
Caduca: 7 de abril de 2027


          Lacre: protocolo de mensajería cifrada entre iguales
                               Versión 1
```

## Resumen

Este documento describe Lacre, el protocolo de la aplicación p2message.
Lacre permite que personas que se identifican solo con claves criptográficas
intercambien mensajes cifrados de extremo a extremo sin ningún servidor
central. Cubre la identidad y los *handles* verificables, la publicación de
identidades en una DHT Kademlia, el transporte sobre libp2p y Tor, los grupos
cifrados con MLS (RFC 9420), el almacenamiento diferido en buzones de otros
nodos con remitente sellado, y el envío de archivos dentro de un grupo.

## Estado de este documento

Borrador de trabajo. Describe la implementación de referencia tal como está
en el repositorio (crates `lacre-core` y `lacre-node`) en la fecha indicada. Si
este documento y el código no coinciden, manda el código y este documento
tiene un error.

Lacre ha pasado una revisión de seguridad interna (D-039 a D-042), pero
ninguna auditoría independiente. La versión 1 del protocolo **no** es estable y puede cambiar
sin mantener la compatibilidad.

«Lacre» es el nombre de trabajo del protocolo. Los identificadores que viajan
por la red (cadenas de separación de dominio, rutas de la DHT, identificadores
de protocolo libp2p) siguen usando `p2message` y `p2m`, y la sección 15
explica por qué no conviene cambiarlos. Hay un proyecto de software libre
anterior llamado Lacre (cifrado de correo con OpenPGP para Postfix, de
Disroot). El nombre definitivo está pendiente.

## Índice

1. Introducción
2. Convenciones y terminología
3. Visión general
4. Identidad
5. Directorio distribuido (DHT)
6. Transporte
7. Mensajes del protocolo directo
8. Grupos (MLS)
9. Contenido de aplicación
10. Entrega y almacenamiento diferido
11. Archivos
12. Sincronización entre dispositivos de una persona
13. Consideraciones de seguridad
14. Consideraciones de privacidad
15. Registro de identificadores
16. Referencias
- Apéndice A. Esquemas CDDL
- Apéndice B. Constantes
- Apéndice C. Relación con el registro de decisiones

---

## 1. Introducción

Casi toda la mensajería actual depende de servidores controlados por un
tercero, que pueden ver los metadatos, ser obligados a colaborar, fallar o
desaparecer. Lacre busca que la comunicación privada solo dependa de quienes
participan en ella:

- **Identidad sin registro.** Una persona *es* una clave Ed25519 que genera
  ella misma. No hay número de teléfono, correo ni cuenta.
- **Sin servidores propios.** Los nodos se encuentran mediante una DHT,
  mDNS, la DHT Mainline de BitTorrent y servicios onion de Tor.
- **Cifrado de extremo a extremo** con MLS, también para grupos y para
  varios dispositivos por persona.
- **Entrega a dispositivos desconectados** mediante buzones en otros nodos,
  que guardan sobres sellados sin poder leer el contenido ni saber quién los
  envía.

### 1.1. Objetivos

1. Una persona puede tener varios dispositivos (hasta 16), y todos reciben
   todos los mensajes.
2. Chats 1:1 y de grupo con el mismo mecanismo.
3. Funcionar detrás de NAT y CGNAT sin servidores públicos dedicados.
4. Ocultar el contenido a todos salvo a los participantes, y reducir los
   metadatos que ven los nodos intermedios.

### 1.2. Fuera del alcance

- El anonimato frente a un adversario global (ver sección 14).
- Las notificaciones push de terceros (APNs, FCM).
- La recuperación de la clave raíz (frase de recuperación; pendiente).
- La interfaz de usuario y el almacenamiento local, salvo cuando afectan a
  la interoperabilidad.

## 2. Convenciones y terminología

Las palabras **DEBE**, **NO DEBE**, **OBLIGATORIO**, **DEBERÍA**, **NO
DEBERÍA**, **RECOMENDADO**, **PUEDE** y **OPCIONAL** se interpretan como
MUST, MUST NOT, REQUIRED, SHOULD, SHOULD NOT, RECOMMENDED, MAY y OPTIONAL en
BCP 14 [RFC2119] [RFC8174], cuando aparecen en mayúsculas.

`||` es la concatenación de cadenas de bytes. `"…"` es una cadena ASCII sin
terminador salvo que se escriba `\0`. `BASE32` es el alfabeto de [RFC4648]
sección 6, **sin relleno y en minúsculas**. `BLAKE3-KDF(ctx, ikm)` es la
función `derive_key` de BLAKE3 con contexto `ctx` y material `ikm` (salida de
32 bytes). Los tiempos son segundos desde 1970-01-01T00:00:00Z.

Términos:

- **Persona**: quien usa Lacre. Tiene una clave raíz.
- **Clave raíz** (`root_pk`): clave Ed25519 que identifica a la persona y
  firma su registro de identidad.
- **Dispositivo**: instancia de Lacre con su propia clave Ed25519
  (`device_pk`). Es a la vez su identidad de red y su clave de firma MLS.
- **Dispositivo primario**: el que guarda la clave raíz. Es el único que puede
  añadir o revocar dispositivos.
- **Handle**: nombre legible `nombre#sufijo` cuyo sufijo se deriva de la clave
  raíz.
- **Registro de identidad**: documento firmado con la clave raíz que lista los
  dispositivos autorizados.
- **Grupo**: grupo MLS. Cada hoja es un dispositivo, y un chat 1:1 es un grupo
  con los dispositivos de dos personas.
- **Committer**: el único dispositivo autorizado a hacer commits en un grupo.
- **Buzón**: almacén de un nodo que guarda sobres para dispositivos
  desconectados.
- **Sobre**: mensaje sellado para un dispositivo, en el que solo se ve el
  destinatario.
- **Mensajero**: conexión de un solo uso, con una identidad efímera, que se
  usa para depositar sobres.

## 3. Visión general

### 3.1. Capas

```
+-------------------------------------------------------------+
|  Aplicación: texto, stickers, ofertas de archivo, avatares   |  §9, §11
+-------------------------------------------------------------+
|  MLS (RFC 9420): grupos de dispositivos, committer único     |  §8
+-------------------------------------------------------------+
|  Protocolo directo /p2m/msg/1 (request-response CBOR)        |  §7
|     · directo al dispositivo, o                              |
|     · sellado en un sobre y depositado en buzones            |  §10
+-------------------------------------------------------------+
|  libp2p: Noise + yamux, Kademlia /p2m/kad/1.0.0, identify    |  §5, §6
+-------------------------------------------------------------+
|  TCP · QUIC · circuit relay v2 · servicios onion de Tor      |  §6
+-------------------------------------------------------------+
|  Encuentro: mDNS · DHT Mainline/trackers UDP · QR · caché    |  §6.5
+-------------------------------------------------------------+
```

### 3.2. Flujo típico

```
 Alice (dispositivo A1)                 DHT                 Bob (B1, B2)
   |                                     |                        |
   |-- GET /p2m/v1/id/<sufijo de Bob> -->|                        |
   |<---------- registro firmado --------|                        |
   |   (verifica firma y sufijo)         |                        |
   |-- KeyPackage? ------------------------------------------> B1 |
   |<------------------------------------------- KeyPackage --- B1 |
   |   (B2 desconectado: GET /p2m/v1/kp/<B2> en la DHT)           |
   |   crea grupo, Add(B1, B2), commit                            |
   |-- Welcome + ChatMeta ------------------------------------> B1 |
   |-- Welcome en sobre --> buzones cercanos a /p2m/v1/mbox/<B2>  |
   |-- Mls(aplicación) ---------------------------------------> B1 |
   |                       ...B2 vuelve a conectar...             |
   |              buzón -- Deliver{sobres} ------------------> B2 |
```

## 4. Identidad

### 4.1. Claves

Toda clave pública es Ed25519 [RFC8032] de 32 bytes.

- Cada persona genera una **clave raíz** con un generador aleatorio seguro.
  La clave raíz privada NO DEBE salir del dispositivo primario.
- Cada dispositivo genera su propia **clave de dispositivo**, que NO DEBE
  compartir con otros dispositivos.

La clave de dispositivo tiene tres usos: es la identidad libp2p del nodo
(§6.1), la clave de firma de su hoja MLS (§8.2) y la clave con la que se
sellan los sobres dirigidos a él (§10.2).

### 4.2. Handles

Un handle tiene la forma `nombre#sufijo`.

- `nombre`: de 3 a 32 caracteres de `[a-z0-9_]`.
- `sufijo`: 8 caracteres BASE32 (`[a-z2-7]`), derivados de la clave raíz:

```
sufijo = BASE32( Argon2id( password = root_pk,
                           salt     = "p2message/handle-suffix/v1",
                           m = 65536 KiB, t = 3, p = 1,
                           versión = 0x13, salida = 5 bytes ) )
```

El sufijo tiene 40 bits. Fabricar una clave raíz cuyo sufijo coincida con uno
dado cuesta del orden de 2^39 evaluaciones de Argon2id (cada una tarda unos
100-300 ms y usa 64 MiB).

Al **interpretar** un handle escrito por una persona, la implementación DEBE
quitar los espacios de los extremos y una `@` inicial, pasar a minúsculas y
rechazar lo que no cumpla la gramática anterior.

El nombre **no es único**: dos personas pueden llamarse `ana`. Lo que
identifica a alguien es el sufijo, y la sección 4.3 obliga a que el nombre
coincida con el del registro firmado.

### 4.3. Registro de identidad

```
IdentityRecord = {
  v:       1,
  root_pk: Bytes32,
  name:    tstr,             ; nombre del handle
  devices: [ 1*16 DeviceEntry ],
  seq:     uint              ; crece en cada cambio
}
DeviceEntry = { device_pk: Bytes32, label: tstr }

SignedRecord = {
  body: ByteList,            ; CBOR de IdentityRecord
  sig:  ByteList             ; firma Ed25519, 64 bytes
}
```

(`Bytes32` y `ByteList` se definen en el apéndice A.)

La firma se calcula con la clave raíz sobre:

```
"p2message/identity-record/v1\0" || body
```

Quien **verifique** un `SignedRecord` DEBE:

1. Rechazarlo si su codificación ocupa más de 8 KiB.
2. Decodificar `body` y comprobar que `v == 1`.
3. Verificar la firma con `root_pk`.
4. Comprobar que `devices` tiene entre 1 y 16 entradas.
5. Comprobar que `name` es válido y calcular el handle `name#sufijo(root_pk)`.

Al **resolver** un handle, además DEBE exigir que el handle calculado sea el
buscado. De varios registros válidos para el mismo handle, gana el de mayor
`seq`.

Cada vez que el primario añade o quita un dispositivo, incrementa `seq` y
vuelve a firmar y publicar el registro.

### 4.4. Códigos y QR

- **Código de dispositivo**: `BASE32(device_pk)` (52 caracteres). Identifica
  un dispositivo (por ejemplo, para desvincularlo).
- **Código de vincular** (§12.1):

```
link-code = BASE32(device_pk) ":" BASE32(secret) *( "@" multiaddr )
```

  `secret` son 16 bytes aleatorios de un solo uso que el dispositivo nuevo
  genera y guarda 10 minutos. Un código sin secreto NO DEBE aceptarse.
- **QR de contacto**:

```
qr        = "p2m:" handle ":" BASE32(root_pk) *( "@" multiaddr )
```

Las direcciones del QR son las del dispositivo que se pueden marcar desde
cualquier red: `/onion3/…` o IPv4 públicas, nunca direcciones de relay ni
privadas. Quien lee el QR DEBE comprobar que la clave raíz del registro
resuelto es la del QR, y DEBE ignorar las direcciones que no entienda.

## 5. Directorio distribuido (DHT)

### 5.1. Kademlia

Los nodos forman una DHT Kademlia de libp2p con el identificador de protocolo
`/p2m/kad/1.0.0`, separada de la DHT pública de IPFS. Todos los nodos
funcionan en modo servidor. La distancia es la XOR de los SHA-256 de las
claves, como en libp2p.

### 5.2. Espacio de claves

| Clave (bytes)                          | Valor                         |
|----------------------------------------|-------------------------------|
| `"/p2m/v1/id/" \|\| sufijo`            | `SignedRecord` (§4.3)         |
| `"/p2m/v1/kp/" \|\| BASE32(device_pk)` | KeyPackage MLS de último recurso (§8.3) |
| `"/p2m/v1/mbox/" \|\| device_pk`       | *(ningún valor)*: punto de encuentro de los buzones (§10.3) |
| `"/p2m/v1/name/…"`                     | Reservado para handles cortos |

En `/p2m/v1/mbox/` se concatenan los 32 bytes **en crudo** de `device_pk`,
no su BASE32.

### 5.3. Validación de lo que se guarda

Un nodo DEBE validar cada registro antes de guardarlo y DEBE descartar los que
no cumplan una de estas dos reglas:

- Es un `SignedRecord` válido (§4.3) y la clave bajo la que llega es
  exactamente `/p2m/v1/id/<sufijo calculado>`.
- Ocupa como mucho 4 KiB, es un KeyPackage MLS válido (§8.3) marcado como
  de último recurso, y la clave es `/p2m/v1/kp/<BASE32 del device_pk de su
  credencial>`.

Si ya guarda un registro de identidad con `seq` mayor o igual, NO DEBE
sustituirlo. Los KeyPackages válidos siempre sustituyen al anterior.

### 5.4. Publicación

Cada dispositivo vuelve a publicar su registro y su KeyPackage de último
recurso en cada tarea periódica (cada 30 s, o cada 12 min en reposo), con
quórum 1. Las búsquedas reúnen copias durante 1,5 s más tras encontrar la
primera, para dar ocasión a una con `seq` mayor.

## 6. Transporte

### 6.1. Identidad de red

La identidad libp2p de un dispositivo es su clave de dispositivo, así que
`PeerId = multihash(identity, protobuf(Ed25519 device_pk))`. Por eso se pasa
de un `device_pk` a un `PeerId` sin consultar ningún directorio, y cada
conexión autentica la clave del dispositivo con Noise. Las secciones 7 y 10
llaman **par autenticado** a la clave de dispositivo que Noise ha verificado
en la conexión.

### 6.2. Pila

- Transportes: TCP, QUIC v1 y servicios onion v3 de Tor (§6.3), y circuitos
  de relay v2 de libp2p.
- Sobre TCP y Tor: multistream-select v1, Noise XX y yamux.
- Comportamientos: Kademlia (§5), identify (`/p2m/id/1`), request-response
  CBOR (`/p2m/msg/1`), mDNS, cliente de relay, DCUtR y UPnP. Los nodos
  lanzados con `--relay-server` también actúan como servidor de relay.
- Las conexiones sin actividad se cierran a los 120 s y el plazo para
  establecer una conexión es de 60 s.

### 6.3. Tor

Cada dispositivo DEBERÍA publicar un servicio onion v3 (Arti), con puerto
virtual **4001**, y anunciar su dirección como
`/onion3/<56 caracteres BASE32>:4001` mediante identify y la DHT. Así se le
puede llamar aunque esté detrás de CGNAT. La clave del servicio onion es
distinta de la del dispositivo. La implementación de referencia (D-044) la
deriva de ella y no la guarda en disco:

```
semilla   = BLAKE3-KDF("p2message/onion-id/v1", device_sk)
onion_key = Ed25519 con semilla `semilla` (forma expandida, como pide Arti)
```

Conocer la clave pública del dispositivo no permite calcular la dirección
onion. Otras implementaciones PUEDEN generar la clave onion como quieran.

### 6.4. NAT

Los nodos detrás de NAT reservan hueco en hasta 2 relays a la vez, e intentan
pasar a conexión directa con DCUtR y abrir puertos con UPnP. Un relay de Lacre
admite 512 reservas de 1 h, y circuitos de hasta 30 min y 64 MiB, porque
también lleva tráfico cuando el hole punching falla.

### 6.5. Encuentro y arranque

Un nodo conoce a otros por estas vías, en orden de preferencia:

1. **Caché de peers**: hasta 64 peers con hasta 4 direcciones cada uno
   (onion o IPv4 pública). Se vuelven a marcar al arrancar y cada 30 s
   mientras no haya conexión.
2. **mDNS** en la red local (solo con wifi y en primer plano).
3. **Direcciones de un QR** (§4.4).
4. **Punto de encuentro global**: con menos de 3 conexiones, el nodo busca
   relays en la DHT **Mainline** de BitTorrent [BEP5] bajo el infohash

   ```
   topic = BLAKE3-KDF("p2message/v1 rendezvous relays", "")[0..20]
   ```

   y, como respaldo, en trackers UDP públicos [BEP15] con el mismo infohash.
   Los relays se anuncian ahí cada 15 min con su puerto TCP. Quien busca
   marca hasta 8 de las direcciones `/ip4/…/tcp/…` que encuentra. Las
   entradas falsas solo cuestan una llamada, porque la identidad la comprueba
   Noise.

### 6.6. Conexiones vivas

Mientras está en primer plano, un dispositivo DEBERÍA enviar `Ping` (§7) cada
45 s a los demás dispositivos de sus chats, para que la conexión (y su
circuito Tor) no se cierre. Si un dispositivo no responde, espera entre 60 s
y 15 min, doblando el tiempo en cada fallo.

## 7. Mensajes del protocolo directo

### 7.1. Codificación

`/p2m/msg/1` es un protocolo request-response de libp2p: cada petición abre
un flujo, lleva un `Request` y recibe un `Response`, ambos en CBOR [RFC8949].
Una petición ocupa como mucho 1 MiB, una respuesta 10 MiB, y el plazo de
respuesta es de 30 s.

Los tipos son enumeraciones con **etiqueta externa** (convención de serde):
una variante sin datos se codifica como su nombre en texto (`"Fetch"`), y una
variante con datos como un mapa de una sola entrada `{ "Nombre": datos }`.
Las estructuras son mapas cuyas claves son los nombres de los campos en texto.
El apéndice A tiene el esquema completo.

### 7.2. Peticiones

| Petición          | Quién la envía y para qué                         | Respuesta         |
|-------------------|---------------------------------------------------|-------------------|
| `KeyPackage`      | Un committer, para pedir un KeyPackage nuevo      | `KeyPackage(kp)`  |
| `Welcome{welcome, meta}` | El committer, al añadir el dispositivo a un grupo | `Ack`      |
| `Mls(bytes)`      | Un miembro del grupo: mensaje MLS (aplicación o commit) | `Ack`       |
| `LinkGrant{record, secret}` | El primario, al vincular un dispositivo nuevo (§12.1) | `Ack`        |
| `SyncHint{group_id}` | Un miembro, para pedir al committer que sincronice | `Ack`         |
| `Deposit(env)`    | Cualquiera (normalmente un mensajero), para dejar un sobre | `Ack`     |
| `Fetch`           | El destinatario, para pedir sus sobres            | `Envelopes{list, more}` |
| `AckEnvelopes{ids}` | El destinatario, para confirmar lo recibido     | `Ack`             |
| `Deliver{list}`   | Un buzón, para entregar sobres a su destinatario  | `Taken{ids}`      |
| `Chunk{file, index, token}` | Un miembro, para pedir un trozo de archivo (§11) | `Chunk(bytes)` |
| `Ping`            | Cualquiera, para mantener viva la conexión        | `Ack`             |
| `Avatar`          | Un miembro de un chat común, para pedir la foto de perfil | `Avatar{version, image}` |
| `Accepted{groups}` | Otro dispositivo de la misma persona (§12.2)     | `Ack`             |
| `Blocks{list}`    | Otro dispositivo de la misma persona (§12.3)      | `Ack`             |

Cualquier petición se puede rechazar con `Error(tstr)`.

### 7.3. Reglas al recibir

En todas las reglas, el remitente es el par autenticado (§6.1). Si la
petición llega dentro de un sobre (§10.2), el remitente es el `from`
verificado del sobre.

- `Welcome`: el remitente DEBE ser `meta.committer`, y este DEBE ser miembro
  del grupo resultante (§8.4).
- `Mls`: solo se aplican los commits firmados por el committer del grupo
  (§8.5).
- `LinkGrant`: el registro DEBE ser válido e incluir al dispositivo receptor
  y al remitente. `secret` DEBE coincidir con el secreto vigente del código
  de vincular del receptor (comparación en tiempo constante) y se consume al
  usarlo. Si el receptor ya tiene identidad, el registro DEBE ser de la
  misma `root_pk` y su `seq` NO DEBE ser menor que el guardado.
- `Welcome`, `Mls`: la hoja del remitente DEBE tener una credencial que
  corresponda a su clave de firma, y su dispositivo DEBE estar en el
  registro de su `root_pk`. Lo mismo vale para todas las hojas que añade o
  cambia un commit (D-040). Un Welcome solo se acepta sin preguntar si el
  committer está verificado así.
- `Fetch` y `AckEnvelopes` actúan **solo** sobre los sobres cuyo `to` es el
  par autenticado. Nadie más puede leerlos ni borrarlos.
- `Accepted` y `Blocks`: el remitente DEBE ser un dispositivo del registro
  de identidad del receptor.
- `Avatar`: solo se responde a dispositivos que comparten con el receptor un
  chat activo y aceptado, y cuya persona no está bloqueada.
- `Chunk`: solo se responde a dispositivos que son miembros actuales del
  grupo del archivo y presentan el testigo correcto.

## 8. Grupos (MLS)

### 8.1. Parámetros

- MLS 1.0 [RFC9420], suite `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`
  (0x0001). Las demás suites DEBEN rechazarse.
- El árbol de ratchet viaja en los Welcome (extensión `ratchet_tree`).
- Cada miembro conserva las claves de 5 épocas pasadas.

### 8.2. Credenciales

Cada hoja usa una credencial `basic` cuya `identity` es el CBOR de:

```
MemberId = { root_pk: Bytes32, device_pk: Bytes32 }
```

La clave de firma de la hoja DEBE ser igual a `device_pk`. Si no lo es, se
DEBE rechazar el KeyPackage, el Welcome o la hoja.

Que un `MemberId` sea legítimo, es decir, que `device_pk` figure en el
registro de identidad de `root_pk`, se comprueba aparte (§8.6): la
credencial `basic` no lo garantiza por sí sola.

### 8.3. KeyPackages

- **Normales**: de un solo uso. Se piden con `KeyPackage` al dispositivo
  cuando está en línea.
- **De último recurso**: reutilizables. Llevan la extensión `last_resort` y
  declaran que la entienden en sus capacidades. Cada dispositivo publica uno
  en la DHT (§5.2) y lo renueva cada 7 días, para que puedan añadirlo a un
  grupo mientras está desconectado.

Quien obtiene un KeyPackage DEBE validarlo (firma, suite, coherencia de la
credencial) y comprobar que su `MemberId` es el esperado.

### 8.4. Creación de grupos y metadatos

El dispositivo que crea el grupo es su **committer**. Los metadatos del chat
viajan fuera de MLS, junto al Welcome:

```
ChatMeta = {
  title:        tstr / null,
  participants: [ * tstr ],   ; handles de las personas (no dispositivos)
  committer:    Bytes32       ; device_pk del committer
}
```

Entre dos personas hay como mucho un chat 1:1 sin título: si ya existe, no se
crea otro.

Al recibir un `Welcome`, el dispositivo:

1. Comprueba que el remitente es `meta.committer`.
2. Une el grupo. Si ya tenía un grupo con ese `group_id` (lo habían
   expulsado), solo lo sustituye si el committer es el mismo.
3. Comprueba que todas las hojas tienen credenciales coherentes (§8.2) y que
   `meta.committer` es miembro.
4. Verifica los miembros (§8.6).

Un Welcome de alguien con quien no se ha hablado antes crea una **solicitud
de chat**. La implementación no muestra sus imágenes ni descarga sus archivos
hasta que la persona la acepta.

### 8.5. Committer único

Sin un servidor que ordene los mensajes, dos commits simultáneos bifurcarían
el grupo. Por eso:

- Solo el committer hace commits (altas, bajas, cambios de época).
- Al procesar un commit, un miembro DEBE rechazarlo sin aplicarlo si no lo
  firmó el dispositivo `meta.committer`.
- Los envíos de cada dispositivo se serializan, para que un commit salga
  antes que los mensajes de la época siguiente.
- Un mensaje MLS que no se puede procesar (llegó antes que su commit o que el
  Welcome) se guarda y se reintenta tras el siguiente commit, hasta 5 veces y
  con un máximo de 128 en espera.

Cuando el committer sincroniza un grupo (cada 30 s, antes de cada envío suyo
y al recibir un `SyncHint`), hace lo siguiente:

1. Para cada participante de `meta.participants`, resuelve su registro y
   reúne los `MemberId` que deberían estar.
2. Si no consigue resolver a un participante, **no toca** a sus
   dispositivos: identifica sus hojas por el sufijo y las deja. Así no se
   expulsa a nadie por un fallo de red.
3. Expulsa las hojas que sobran (dispositivos revocados o personas que ya no
   participan) y envía el commit a **todos** los miembros anteriores, también
   a los expulsados, para que sepan que salen.
4. Pide KeyPackages a los dispositivos que faltan: primero uno nuevo y, si
   no está en línea, el de último recurso de la DHT. Añade los que consiga,
   envía el commit a los miembros existentes y el `Welcome` con `ChatMeta` a
   los nuevos.

### 8.6. Verificación de miembros

Tras unirse a un grupo o aplicar un commit, cada miembro DEBERÍA resolver los
registros de los participantes y comprobar que cada hoja figura en el registro
de su persona. Si una hoja no figura, la implementación actual **avisa** a la
persona (`UnverifiedMember`) pero no la rechaza. Ver §13.6.

### 8.7. Cambios de participantes y salida

- **Añadir o quitar personas**: el committer cambia `participants`, envía un
  mensaje de aplicación con el nuevo `meta` (§9) y sincroniza (§8.5). Los
  miembros DEBEN ignorar un `meta` que no venga del committer, salvo el
  relevo descrito abajo.
- **Traspasar la gestión**: el committer envía un `meta` con otro
  `committer`. Los miembros DEBEN comprobar que el nuevo es una hoja del
  grupo cuya raíz es de uno de los `participants`. Desde ese mensaje solo
  aceptan commits del nuevo; los que llegaron antes esperan en la cola de
  reintentos (§8.5). Si el committer sale del grupo, primero traspasa la
  gestión al primer participante con dispositivos en el grupo (en el mismo
  `meta`, sin él y con `leave = true`), y es el nuevo quien lo expulsa.
- **Relevo de un committer revocado**: si el dispositivo committer ya no está
  en el registro de su persona, el dispositivo de esa persona con la
  `device_pk` más baja de entre los que siguen en el grupo y en el registro
  envía un `meta` con `committer` = él mismo y `takeover` = su
  `SignedRecord`. Cada miembro DEBE comprobar: que el registro es válido y de
  la raíz del committer y del remitente; que no incluye al committer y sí al
  remitente; que el remitente es el de clave más baja; que `participants` y
  `title` no cambian; y que no tiene ya un registro de esa raíz con mayor
  `seq` que siga incluyendo al committer. Después el nuevo committer
  sincroniza y expulsa al revocado.
- **Salir**: el miembro envía un mensaje de aplicación con `leave = true` y
  sin `meta`. El committer lo quita de `participants`, envía el nuevo `meta`
  con `leave = true` y sincroniza. Si el mensaje de salida lo envió otro
  dispositivo de la misma persona, el receptor marca el chat como abandonado.

## 9. Contenido de aplicación

El texto de cada mensaje de aplicación MLS es el CBOR de:

```
AppMessage = {
  text:     tstr,             ; vacío si lleva meta, sticker o file
  sent_at:  uint,             ; hora declarada por el remitente
  ? meta:    ChatMeta,        ; solo del committer (§8.7)
  ? sticker: bstr,            ; imagen ≤ 200 KiB (§9.1)
  ? file:    FileOffer,       ; oferta de archivo (§11)
  ? avatar:  uint,            ; versión de la foto de perfil (≥ 1)
  ? leave:   bool,            ; solo si es true
  ? takeover: bstr            ; SignedRecord del relevo (§8.7)
}
```

Los campos opcionales se omiten si no hacen falta. El receptor DEBE ignorar
los campos que no conozca. Un mensaje sin `text`, `sticker`, `file` ni `meta`
solo anuncia una foto de perfil nueva y no se muestra.

### 9.1. Stickers

Un sticker es una imagen PNG, GIF, WebP o JPEG de como mucho 200 KiB, que
viaja dentro del mensaje cifrado. Antes de decodificarla, el receptor DEBE
comprobar en la cabecera que no pasa de 1024 px de lado, para evitar bombas
de descompresión.

### 9.2. Fotos de perfil

`avatar` anuncia la versión de la foto de quien envía. El receptor la pide
con la petición `Avatar` y recibe `{version, image}` (imagen de como mucho
128 KiB y 512 px de lado, o `null` si no hay foto).

## 10. Entrega y almacenamiento diferido

### 10.1. Envío a un grupo

Para entregar una petición (un mensaje MLS o un Welcome) a una lista de
dispositivos, el remitente, para cada dispositivo distinto de sí mismo:

1. Envía la petición **directamente**. Si recibe `Ack`, ya está entregada.
2. Si no, la **sella** en un sobre y la **deposita** en buzones (§10.3).

### 10.2. Sobres (remitente sellado)

```
Envelope = {
  to:     Bytes32,            ; device_pk destinatario (lo único visible)
  sealed: bstr                ; eph (32) || texto cifrado
}
```

Para sellar `payload` para `to`:

```
R      = Montgomery(to)                      ; Ed25519 → X25519
e      = 32 bytes aleatorios;   E = X25519(e, 9)
S      = X25519(e, R);  si S = 0^32 → error
k      = BLAKE3-KDF("p2message/envelope/v3 seal", E || R || S)
sent   = hora actual, segundos Unix (uint)
sig    = Ed25519-Sign(sender, "p2message/envelope/v3\0" || to || E
                              || u64be(sent) || payload)
inner  = CBOR({ from: sender_pk (Bytes32), sent: uint, sig: bstr, payload: bstr })
sealed = E || ChaCha20-Poly1305(k, nonce = 0^12, aad = "", inner)
```

El nonce puede ser fijo porque la clave `k` es única para cada sobre, ya que
la clave efímera es nueva cada vez.

Al abrir un sobre, el destinatario comprueba que `to` es su clave, calcula
`S = X25519(escalar(device_sk), E)`, deriva `k`, descifra, y verifica `sig`
con `from` sobre `to || E || u64be(sent) || payload`. Como la firma cubre `to`
y `E`, nadie puede volver a sellar un contenido firmado para otro
destinatario; como cubre `sent`, nadie puede cambiar su fecha.

El destinatario DEBE rechazar un sobre con `sent + 15 días < ahora` (más
viejo que lo que guarda un buzón, §10.4) o con `sent > ahora + 1 día`
(reloj del remitente demasiado adelantado).

- `payload` es el CBOR de una `Request`. Dentro de un sobre solo se admiten
  `Welcome`, `Mls`, `SyncHint`, `Accepted` y `Blocks`. El resto DEBE
  rechazarse.
- Una vez abierto, el contenido se procesa como si el remitente `from` lo
  hubiera enviado directamente (§7.3).
- El identificador del sobre es `BLAKE3(sealed)`. El destinatario recuerda
  los identificadores, con su `sent`, de los sobres abiertos que aún no han
  caducado, y descarta los repetidos. Si pasan de 20 000, olvida el de
  `sent` más antiguo y desde entonces rechaza todo sobre con `sent` igual o
  anterior (un «suelo»). Así ninguna repetición se acepta: o el id sigue
  recordado, o el sobre ya caducó, o queda por debajo del suelo.
- Antes de guardar un sobre, un buzón (que no puede descifrarlo) solo
  comprueba que `to` es una clave Ed25519 válida y que `sealed` mide al
  menos 48 bytes y como mucho 256 KiB.

### 10.3. Depósito

Los buzones de un dispositivo `D` son los nodos más cercanos en la DHT a
`"/p2m/v1/mbox/" || D`. Para depositar un sobre para `D`, el remitente:

1. Guarda una copia en su propio buzón.
2. Busca los nodos más cercanos a la clave de buzón de `D`, descarta a `D`
   y se queda con 3.
3. Les envía `Deposit(env)` mediante el **mensajero**: un nodo libp2p
   temporal con una **identidad nueva y de un solo uso**, que solo envía
   peticiones. Si hay Tor y el buzón tiene dirección onion, usa solo esa,
   con circuitos aislados de los del nodo. Así, ni el sobre ni la conexión
   ni (con Tor) la IP revelan al remitente.
4. Si así no consigue guardar ninguna copia, vuelve a depositar con su
   identidad normal.

### 10.4. Buzón

Un buzón DEBE aplicar límites por destinatario, porque no sabe quién envía
cada sobre. La implementación usa estos:

| Límite                         | Valor        |
|--------------------------------|--------------|
| Tamaño de `sealed`             | ≤ 256 KiB    |
| Sobres por destinatario        | 500 (si se llena, se descarta el más antiguo) |
| Destinatarios distintos        | 2000         |
| Tiempo máximo guardado         | 14 días      |
| Bytes por respuesta a `Fetch`  | 4 MiB, con `more = true` si quedan más |
| Bytes por `Deliver`            | 768 KiB      |

Los sobres repetidos (con el mismo identificador) se guardan una sola vez.

### 10.5. Recogida (pull)

Un dispositivo recoge sus sobres con `Fetch` de cada peer al que se conecta
y, en cada tarea periódica, de los nodos más cercanos a su clave de buzón y
de los que tiene conectados. Por cada lote procesa los sobres, responde con
`AckEnvelopes` (el buzón los borra) y repite mientras `more` sea verdadero,
hasta 16 veces.

### 10.6. Entrega activa (push)

Un buzón también intenta llevar los sobres a su destinatario con
`Deliver{list}`, buscando su dirección (onion incluida) en la DHT:

- El primer intento es entre 60 y 90 s después del depósito. El retraso
  aleatorio evita delatar el momento exacto del depósito.
- Si falla, reintenta a los 5 min, 30 min y después cada 2 h, hasta que el
  sobre caduca.
- El receptor acepta como mucho 300 sobres de cada peer cada 10 min, ignora
  los que no son para él y responde `Taken{ids}` con los que ya no hay que
  guardar, también los que no pudo abrir. El buzón borra esos.

## 11. Archivos

Un archivo (de como mucho 100 MiB) no viaja dentro de MLS. Por MLS solo va
una **oferta**:

```
FileOffer = {
  name:   tstr,
  size:   uint,
  key:    Bytes32,            ; clave aleatoria del archivo
  hashes: bstr                ; BLAKE3 de cada trozo cifrado, 32 bytes cada uno
}
```

- El archivo se parte en trozos de 256 KiB. El trozo `i` se cifra con
  ChaCha20-Poly1305 usando `key` y un nonce de 12 bytes cuyos 4 primeros son
  `i` en little-endian y el resto ceros. El cifrado es determinista, así que
  cualquier dispositivo que tenga el archivo sirve exactamente los mismos
  bytes.
- `id = hex(BLAKE3-KDF("p2message 2026 file id", key))`
- `token = BLAKE3-KDF("p2message 2026 file token", key)`
- Para pedir el trozo `i` se envía `Chunk{file: id, index: i, token}`. Se
  pregunta primero al remitente y después a otros miembros del grupo.
- Quien sirve el trozo DEBE comprobar el testigo y que quien lo pide es
  miembro actual del grupo.
- Quien lo recibe DEBE comprobar la longitud y el hash del trozo **antes** de
  descifrarlo.

## 12. Sincronización entre dispositivos de una persona

### 12.1. Vinculación

1. El dispositivo nuevo genera su clave y muestra su código de vincular
   (§4.4).
2. El primario añade el dispositivo a su registro (incrementando `seq`), lo
   publica y envía `LinkGrant{record, secret}` al dispositivo nuevo, con el
   secreto que leyó del código de vincular (§4.4).
3. El dispositivo nuevo comprueba el registro y el secreto (§7.3) y adopta
   esa identidad.
4. El primario sincroniza los grupos de los que es committer y envía
   `SyncHint` a los committers de los demás.

Para revocar un dispositivo, el primario lo quita del registro y lo publica.
Cada committer lo expulsa en su siguiente sincronización. El primario no
puede revocarse a sí mismo.

### 12.2. Chats aceptados

`Accepted{groups}` comunica a los demás dispositivos de la persona qué chats
ha aceptado. Si un grupo aún no se conoce, se guarda como aceptado de
antemano, para cuando llegue su Welcome.

### 12.3. Bloqueos

`Blocks{list}` es un mapa `sufijo → {handle, blocked, at}`. Al combinarlo con
el propio, gana para cada sufijo la entrada con `at` mayor (la última
escritura). Los desbloqueos también se recuerdan, para que una lista vieja no
los deshaga. Los mensajes de personas bloqueadas se descartan al recibirlos.

## 13. Consideraciones de seguridad

### 13.1. Suplantación de handles

Con un sufijo de 40 bits, suplantar a una persona *concreta* cuesta del orden
de 2^39 evaluaciones de Argon2id (64 MiB y 3 pasadas cada una). Encontrar
*alguna* colisión entre dos personas cualesquiera es mucho más barato (efecto
cumpleaños, unas 2^20 evaluaciones). Por eso, quien guarda un contacto DEBERÍA
fijar su `root_pk` y avisar si cambia. El QR (§4.4) transmite la clave
completa y evita este riesgo.

La implementación de referencia (D-041) fija la `root_pk` de cada contacto
y de la propia identidad, y trata como error un QR con otra raíz. Si para
un handle sin fijar hay registros válidos de raíces distintas, no elige
ninguno («handle ambiguo»). Verifica como mucho 16 candidatos.

Para comprobar la raíz de un contacto sin QR, las dos partes comparan su
**número de seguridad** (D-043):

```
o      = BLAKE3-XOF(derive_key "p2message/safety-number/v1",
                    min(root_a, root_b) || max(root_a, root_b))[0..60]
grupo_i = u40be(o[5i .. 5i+5]) mod 100000,  i = 0..11   ; 5 cifras con ceros
número = grupo_0 " " grupo_1 " " … " " grupo_11
```

El orden se toma comparando los 32 bytes de cada clave lexicográficamente.
Si los dos números coinciden, cada parte tiene fijada la raíz de la otra.

### 13.2. Pérdida o robo de la clave raíz

Quien tenga la clave raíz puede añadir dispositivos al registro y, así, entrar
en todos los grupos de la persona en la siguiente sincronización.

La clave raíz se puede anotar como **frase de recuperación** (D-048): sus 32
bytes como 24 palabras BIP39 (lista inglesa, 8 bits de suma de
comprobación). Con ella, otro dispositivo busca en la DHT el último registro
de esa raíz (`dht_key(sufijo)`, filtrando por `root_pk`), se añade y lo
firma con `seq = max(seq_anterior + 1, ahora_unix)`, opcionalmente quitando
los demás dispositivos. Para pasar el papel de primario se recupera en el
nuevo y se borra la clave raíz del anterior.

La clave raíz no se puede rotar: el sufijo del handle depende de ella. Si
se filtra (el dispositivo primario o la frase), quien la tenga puede firmar
registros con `seq` mayores. La única salida es una identidad nueva.

### 13.3. Vinculación

El `LinkGrant` solo se acepta con el secreto de un solo uso del código de
vincular (D-039). Quien vea ese código en sus 10 minutos de vida puede
adelantarse al dueño. El código fallaría después al dueño, que lo notaría.

Al terminar, los dos dispositivos muestran un código de confirmación
(D-047), que la persona compara:

```
h = BLAKE3(derive_key "p2message/link-confirm/v1", raíz || dispositivo || secreto)
código = u32be(h[0..4]) mod 1000000, como "ddd ddd"
```

`secreto` es el del código de vincular o el token de la invitación. Si no
coinciden, alguien ha cambiado el QR o se ha adelantado. Además, tras
vincular, la lista de dispositivos muestra todos los del registro, y el
primario puede desvincular uno desconocido.

### 13.4. Committer único

Si el committer desaparece, el grupo deja de poder cambiar: no se pueden
añadir ni quitar personas o dispositivos, ni salir, aunque los mensajes
siguen funcionando. Un committer malicioso puede añadir hojas, pero cada
miembro las coteja con los registros (§8.6).

### 13.5. Una clave para varios usos

La clave de dispositivo se usa para Noise, para firmas MLS y para sellar
sobres (como X25519). Cada uso separa su dominio, pero conviene revisarlo
antes de una versión estable.

### 13.6. Miembros no autorizados

Desde D-040 se rechazan los commits con hojas cuya credencial no corresponda
a su clave de firma. Los mensajes de un dispositivo que se sabe ajeno a la
raíz que dice ser se tratan como falsificados. La verificación de §8.6 con el
registro resuelto de la DHT sigue solo avisando: un registro que no se pudo
resolver puede dejar en el grupo una hoja aún sin comprobar.

### 13.7. Denegación de servicio

- Comprobar un registro cuesta un Argon2id (memorizado por clave). Desde
  D-042:
  - cada peer puede pedir 20 validaciones cada 10 min;
  - hay como mucho 2 a la vez;
  - un registro idéntico al guardado no se revalida;
  - las firmas, el nombre y los dispositivos se comprueban antes de
    Argon2.

  Un atacante con muchas identidades de libp2p puede multiplicar ese cupo.
- Los buzones limitan por destinatario a 500 sobres y 16 MiB, y en total a
  256 MiB. Un atacante puede llenar el buzón de una víctima y desplazar sobres
  legítimos. Para compensarlo, el remitente guarda su propia copia y deposita
  en 3 buzones.
- `KeyPackage` responde a cualquiera y genera material de clave cada vez. Se
  limita a 8 por peer y 100 en total por hora.
- El nodo acepta como mucho 64 peticiones simultáneas y limita las conexiones
  entrantes (D-042).

### 13.8. Repetición y orden

MLS protege contra la repetición de mensajes de grupo. Los sobres llevan una
fecha de sellado firmada y caducan a los 15 días; dentro de ese plazo se
deduplican por identificador, que se marca como visto solo después de abrir
bien el sobre (§10.2). Un buzón puede retrasar un sobre hasta que caduque,
pero no repetirlo ni cambiarle la fecha. La fecha depende del reloj del
remitente, así que `sent_at` (y `sent`) no son fiables para ordenar
mensajes.

### 13.9. Secreto hacia delante de los sobres

Los sobres usan una clave efímera del remitente contra la clave estática del
destinatario: si se compromete la clave del dispositivo, se pueden abrir los
sobres guardados que aún existan. El contenido MLS (`Mls`) sigue protegido por
el secreto hacia delante de MLS. `ChatMeta` (en los Welcome), `Accepted` y
`Blocks` dependen solo del sobre.

## 14. Consideraciones de privacidad

- **El PeerId es la clave del dispositivo** (§6.1), y esa clave figura en el
  registro público de la persona. Quien vea una IP conectada con ese PeerId
  puede asociarla a un handle. Tor reduce el problema solo si **todas** las
  conexiones del dispositivo van por onion, cosa que hoy no ocurre (TCP, QUIC
  y mDNS siguen activos).
- **El registro de identidad es público**: nombre, lista de dispositivos y
  sus etiquetas (por ejemplo «móvil»). Las etiquetas DEBERÍAN ser genéricas.
- **Los buzones ven** el destinatario, el tamaño y la hora de cada sobre. Con
  el mensajero y Tor no ven al remitente.
- **Las conexiones vivas** (§6.6) revelan a los nodos intermedios con qué
  dispositivos se mantiene una conexión.
- **El punto de encuentro** en Mainline revela que una IP ejecuta un relay de
  Lacre.
- **Los avatares** solo se sirven a quien comparte un chat aceptado.

## 15. Registro de identificadores

No se pide nada a IANA. Lacre usa estos identificadores, y cambiar
cualquiera de ellos rompe la compatibilidad:

| Identificador                              | Uso                     |
|--------------------------------------------|-------------------------|
| `/p2m/kad/1.0.0`                           | Protocolo Kademlia      |
| `/p2m/msg/1`                               | Protocolo directo       |
| `/p2m/id/1`                                | Versión de identify     |
| `/p2m/v1/id/`, `/p2m/v1/kp/`, `/p2m/v1/mbox/`, `/p2m/v1/name/` | Espacio de claves de la DHT |
| `p2message/handle-suffix/v1`               | Sal del sufijo del handle |
| `p2message/identity-record/v1\0`           | Dominio de la firma del registro |
| `p2message/envelope/v3\0`                  | Dominio de la firma del sobre |
| `p2message/envelope/v3 seal`               | Contexto de la KDF del sobre |
| `p2message/safety-number/v1`               | Contexto del número de seguridad |
| `p2message/link-confirm/v1`                | Contexto del código de confirmación al vincular |
| `p2message/onion-id/v1`                    | Contexto de la clave onion (implementación) |
| `p2message/v1 rendezvous relays`           | Contexto del infohash de encuentro |
| `p2message 2026 file id`, `p2message 2026 file token` | Contextos de la KDF de archivos |
| `p2m:`                                     | Prefijo del QR          |
| Puerto onion 4001                          | Servicio onion          |

**Sobre el cambio de nombre:** la sal del sufijo forma parte de cada handle,
así que cambiar `p2message/handle-suffix/v1` cambiaría los handles de todo el
mundo. Lo RECOMENDADO es mantener estos identificadores aunque cambie el nombre
de la aplicación, igual que, por ejemplo, Signal conserva cadenas de
TextSecure. Si algún día se cambian, que sea en una versión 2 del protocolo,
cambiando todos a la vez.

## 16. Referencias

### 16.1. Normativas

- [RFC2119] Bradner, S., «Key words for use in RFCs to Indicate Requirement
  Levels», BCP 14, RFC 2119, 1997.
- [RFC8174] Leiba, B., «Ambiguity of Uppercase vs Lowercase in RFC 2119 Key
  Words», BCP 14, RFC 8174, 2017.
- [RFC4648] Josefsson, S., «The Base16, Base32, and Base64 Data Encodings»,
  RFC 4648, 2006.
- [RFC7748] Langley, A., et al., «Elliptic Curves for Security», RFC 7748,
  2016.
- [RFC8032] Josefsson, S. e I. Liusvaara, «Edwards-Curve Digital Signature
  Algorithm (EdDSA)», RFC 8032, 2017.
- [RFC8439] Nir, Y. y A. Langley, «ChaCha20 and Poly1305 for IETF
  Protocols», RFC 8439, 2018.
- [RFC8949] Bormann, C. y P. Hoffman, «Concise Binary Object Representation
  (CBOR)», STD 94, RFC 8949, 2020.
- [RFC9106] Biryukov, A., et al., «Argon2 Memory-Hard Function for Password
  Hashing and Proof-of-Work Applications», RFC 9106, 2021.
- [RFC9420] Barnes, R., et al., «The Messaging Layer Security (MLS)
  Protocol», RFC 9420, 2023.
- [BLAKE3] O'Connor, J., et al., «BLAKE3: one function, fast everywhere»,
  2020.
- [libp2p] Especificaciones de libp2p: Noise, yamux, Kademlia, identify,
  circuit relay v2, DCUtR. <https://github.com/libp2p/specs>
- [rend-spec-v3] The Tor Project, «Tor Rendezvous Specification - Version 3».

### 16.2. Informativas

- [RFC8610] Birkholz, H., et al., «Concise Data Definition Language (CDDL)»,
  RFC 8610, 2019.
- [BEP5] «DHT Protocol», BitTorrent Enhancement Proposal 5.
- [BEP15] «UDP Tracker Protocol for BitTorrent», BitTorrent Enhancement
  Proposal 15.
- [DECISIONES] Registro de decisiones de p2message, `docs/DECISIONES.md`.

---

## Apéndice A. Esquemas CDDL

Esquema [RFC8610] de lo que viaja por `/p2m/msg/1`, tal como lo codifica la
implementación de referencia (serde + ciborium).

**Ojo:** según el campo, los bytes viajan de dos formas. Los marcados con
`serde_bytes` son cadenas de bytes CBOR (`bstr`). Los demás campos de bytes
de longitud fija (`[u8; 32]`) y algunos `Vec<u8>` viajan como **listas de
enteros**, que ocupan casi el doble. Es un defecto conocido: unificarlo como
`bstr` es un cambio incompatible que conviene hacer antes de estabilizar.

```cddl
Bytes32  = [ 32*32 uint .size 1 ]   ; lista de 32 enteros 0..255
ByteList = [ * uint .size 1 ]       ; lista de enteros 0..255

Request = "KeyPackage"
        / { "Welcome":      { welcome: bstr, meta: ChatMeta } }
        / { "Mls":          bstr }
        / { "LinkGrant":    { record: bstr, secret: bstr } } ; record: CBOR de SignedRecord
        / { "SyncHint":     { group_id: ByteList } }
        / { "Deposit":      Envelope }
        / "Fetch"
        / { "AckEnvelopes": { ids: [ * Bytes32 ] } }
        / { "Deliver":      { list: [ * Envelope ] } }
        / { "Chunk":        { file: tstr, index: uint, token: Bytes32 } }
        / "Ping"
        / "Avatar"
        / { "Accepted":     { groups: [ * ByteList ] } }
        / { "Blocks":       { list: { * tstr => BlockEntry } } }

Response = { "KeyPackage": bstr }
         / "Ack"
         / { "Envelopes": { list: [ * Envelope ], more: bool } }
         / { "Chunk":     bstr }
         / { "Avatar":    { version: uint, image: bstr / null } }
         / { "Taken":     { ids: [ * Bytes32 ] } }
         / { "Error":     tstr }

ChatMeta = {
  title:        tstr / null,
  participants: [ * tstr ],
  committer:    Bytes32,
}

Envelope = { to: Bytes32, sealed: bstr }
EnvelopeInner = { from: Bytes32, sent: uint, sig: bstr, payload: bstr }

BlockEntry = { handle: tstr, blocked: bool, at: uint }

SignedRecord   = { body: ByteList, sig: ByteList }
IdentityRecord = {
  v: 1, root_pk: Bytes32, name: tstr,
  devices: [ 1*16 DeviceEntry ], seq: uint,
}
DeviceEntry = { device_pk: Bytes32, label: tstr }

MemberId = { root_pk: Bytes32, device_pk: Bytes32 }

AppMessage = {
  text: tstr,
  sent_at: uint,
  ? meta: ChatMeta,
  ? sticker: bstr,
  ? file: FileOffer,
  ? avatar: uint,
  ? leave: true,
}

FileOffer = { name: tstr, size: uint, key: Bytes32, hashes: bstr }
```

## Apéndice B. Constantes

| Constante                                   | Valor      | Sección |
|---------------------------------------------|------------|---------|
| Longitud del nombre                         | 3-32       | 4.2 |
| Sufijo                                      | 5 bytes / 8 caracteres BASE32 | 4.2 |
| Argon2id del sufijo (m, t, p)               | 64 MiB, 3, 1 | 4.2 |
| Dispositivos por persona                    | 16         | 4.3 |
| Tamaño máximo de un registro                | 8 KiB      | 4.3 |
| Tamaño máximo de un KeyPackage en la DHT    | 4 KiB      | 5.3 |
| Espera extra de una búsqueda en la DHT      | 1,5 s      | 5.4 |
| Plazo de una búsqueda en la DHT             | 20 s       | 5.1 |
| Tarea periódica (normal / reposo)           | 30 s / 12 min | 5.4 |
| Puerto virtual onion                        | 4001       | 6.3 |
| Reservas simultáneas en relays              | 2          | 6.4 |
| Cierre de conexión inactiva                 | 120 s      | 6.2 |
| Plazo para establecer una conexión          | 60 s       | 6.2 |
| Peers en la caché / direcciones por peer    | 64 / 4     | 6.5 |
| Conexiones por debajo de las cuales se busca encuentro | 3 | 6.5 |
| Anuncio en Mainline                         | cada 15 min | 6.5 |
| Ping de conexión viva                       | 45 s (reintento 60 s-15 min) | 6.6 |
| Petición / respuesta máxima                 | 1 MiB / 10 MiB | 7.1 |
| Plazo de respuesta                          | 30 s       | 7.1 |
| Épocas pasadas conservadas                  | 5          | 8.1 |
| Renovación del KeyPackage de último recurso | 7 días     | 8.3 |
| Reintentos de mensajes MLS en espera        | 5 (máximo 128 en espera) | 8.5 |
| Sticker                                     | ≤ 200 KiB, ≤ 1024 px | 9.1 |
| Foto de perfil                              | ≤ 128 KiB, ≤ 512 px | 9.2 |
| Copias remotas de cada sobre                | 3          | 10.3 |
| Plazo del mensajero                         | 90 s       | 10.3 |
| Sobre (`sealed`)                            | 48 B - 256 KiB | 10.2 |
| Sobres por destinatario / destinatarios     | 500 / 2000 | 10.4 |
| Tiempo de vida de un sobre                  | 14 días    | 10.4 |
| `Fetch` / `Deliver` por lote                | 4 MiB / 768 KiB | 10.4 |
| Rondas de recogida o entrega                | 16         | 10.5 |
| Primera entrega activa                      | 60 s + 0-30 s | 10.6 |
| Reintentos de entrega activa                | 5 min, 30 min, 2 h | 10.6 |
| Sobres aceptados por `Deliver`              | 300 por peer cada 10 min | 10.6 |
| Identificadores de sobre recordados (máx.)  | 20 000     | 10.2 |
| Antigüedad máxima de un sobre               | 15 días    | 10.2 |
| Adelanto de reloj tolerado en un sobre      | 1 día      | 10.2 |
| Archivo / trozo                             | 100 MiB / 256 KiB | 11 |

## Apéndice C. Relación con el registro de decisiones

| Sección | Decisiones |
|---------|------------|
| 4 Identidad | D-002, D-003, D-004 |
| 5 DHT | D-005, D-017 |
| 6 Transporte | D-008, D-009, D-018, D-019, D-020, D-021, D-028, D-034 |
| 8 Grupos | D-006, D-007, D-010, D-024, D-027, D-029, D-031 |
| 9 Contenido | D-025, D-030 |
| 10 Entrega | D-016, D-034 |
| 11 Archivos | D-026 |
| 12 Dispositivos | D-011, D-029, D-031 |
