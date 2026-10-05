# Registro de decisiones

Cada decisión lleva su motivo y lo que queda pendiente. Si una decisión se
revisa, no se borra: se marca como **sustituida por D-xxx**.

Estado: prueba de concepto (PoC) en Linux, 2026-10-05.

---

## D-001 · Núcleo en Rust compartido por todas las plataformas

- **Decisión:** toda la lógica (identidad, cifrado, red) vive en crates Rust.
  `p2m-core` no depende de la red; `p2m-node` añade libp2p y una CLI.
- **Motivo:** una única implementación auditada para Android, iOS, Linux,
  Windows y macOS. Las apps llamarán al núcleo por FFI.
- **Interfaz:** ver D-015.

## D-002 · La identidad es la clave pública; el handle solo sirve para encontrarla (opción A)

- **Decisión:** cada persona tiene una **clave raíz** Ed25519. Contactos,
  chats y verificaciones se basan en esa clave, nunca en el texto del handle.
  El handle tiene la forma `nombre#sufijo` y el sufijo se deriva de la clave
  (D-003).
- **Motivo:** así no hace falta consenso global (blockchain) para que el handle
  sea único y seguro.
- **Migración a A+B (handles cortos):** será una capa añadida. En la DHT el
  espacio `/p2m/v1/id/` es de la opción A y `/p2m/v1/name/` queda **reservado**
  para B: registro `nombre → clave raíz` con prueba de trabajo y expiración.
  Los clientes actuales ignoran ese espacio y ningún contacto existente cambia.

## D-003 · Sufijo del handle: Argon2id de la clave raíz, 40 bits

- **Decisión:** `sufijo = base32(Argon2id(root_pk, sal="p2message/handle-suffix/v1",
  m=64 MiB, t=3, p=1, salida=5 bytes))`, que da 8 caracteres en minúscula.
- **Motivo:** un sufijo corto y barato de calcular permitiría fabricar claves
  hasta dar con el sufijo de otra persona. Con 40 bits y un hash lento y con
  mucha memoria, suplantar un handle concreto cuesta unos 2^39 Argon2.
- **El sufijo depende solo de la clave, no del nombre.** Si cambias de nombre,
  conservas el sufijo.
- **Importante:** los parámetros forman parte del protocolo v1. Cambiarlos
  cambia todos los handles, así que exigiría un v2.
- **Más fuerte todavía:** el QR (`p2m:handle:clave`) contiene la clave
  completa de 256 bits y no depende de esta derivación.

## D-004 · Jerarquía de claves: raíz → dispositivos

- **Decisión:** la clave raíz firma un **registro de identidad** con la lista
  de dispositivos autorizados. Cada dispositivo genera su propia clave, que
  nunca sale de él. En la PoC **la clave raíz solo está en el dispositivo
  primario** (el que hizo `init`).
- **Motivo:** permite multidispositivo y revocación sin compartir claves de
  dispositivo. Además, si roban un dispositivo secundario, el ladrón no puede
  añadir dispositivos.
- **Pendiente:**
  - Frase de recuperación (24 palabras) para la clave raíz.
  - Transferir el rol de primario a otro dispositivo.
  - Mientras tanto, si se pierde el primario, los demás dispositivos siguen
    funcionando pero no se pueden vincular dispositivos nuevos.

## D-005 · Registros de identidad en una DHT Kademlia propia

- **Decisión:**
  - DHT Kademlia de libp2p con protocolo propio `/p2m/kad/1.0.0`, separada de
    la DHT de IPFS.
  - El registro se guarda en la clave `/p2m/v1/id/<sufijo>`.
  - Gana el registro válido con mayor `seq`.
  - Los nodos **validan antes de guardar** (`StoreInserts::FilterBoth`): firma,
    sufijo correcto y que no sea más antiguo que el que ya tienen.
- **Motivo:** si los nodos no validaran, cualquiera podría sobrescribir la
  clave de otra persona con basura (Kademlia guarda un registro por clave).
- **Riesgos conocidos:**
  - Ataques Sybil o eclipse: un atacante que controle los nodos más cercanos a
    una clave puede *ocultar* un registro, aunque no falsificarlo.
  - Un nodo malicioso puede servir una versión antigua. Mitigación parcial: el
    cliente compara con lo que ya conoce y se queda con el `seq` mayor.
  - Validar cuesta un Argon2. Se memoriza por clave raíz y se hace fuera del
    bucle de red, pero sigue siendo un posible vector de DoS.
- **Pendiente:** limitar la tasa de escrituras por peer; probar en redes
  grandes.

## D-006 · MLS (RFC 9420) para chats 1:1 y grupos

- **Decisión:**
  - Implementación: `openmls`, suite `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`.
  - **Cada dispositivo es una hoja** del árbol MLS.
  - Un chat 1:1 es un grupo con todos los dispositivos de las dos personas, y
    un grupo normal funciona igual.
  - La credencial de cada hoja es un `MemberId` (clave raíz + clave de
    dispositivo), y la clave de firma de la hoja *debe* ser la del dispositivo
    (se comprueba).
- **Motivo:** MLS resuelve multidispositivo, grupos, secreto hacia adelante y
  recuperación tras compromiso con un único mecanismo. Así no hay que gestionar
  N×M sesiones de Signal.
- **Ojo:** MLS no sabe que un dispositivo pertenece a una raíz. Eso lo
  garantiza la aplicación: el committer solo añade dispositivos que figuran en
  el registro firmado (y su KeyPackage debe coincidir), y quien entra verifica
  los miembros contra la DHT.

## D-007 · Un único *committer* por grupo

- **Decisión:** el dispositivo que crea el grupo es el único que puede hacer
  commits: altas, bajas y cambios de época. Los demás rechazan cualquier commit
  de otro dispositivo sin aplicarlo.
- **Motivo:** sin servidor que ordene los mensajes, dos commits simultáneos
  bifurcarían el grupo. Un único committer elimina ese problema de raíz.
- **Cómo funciona:** el committer sincroniza periódicamente (cada 30 s) y antes
  de cada envío: lee los registros de los participantes, añade los
  dispositivos nuevos y expulsa los revocados. Los demás pueden pedírselo con
  `SyncHint`. Si no consigue resolver a una persona, no toca a sus
  dispositivos, para no expulsar a nadie por un fallo de red.
- **Limitaciones:**
  - Con el committer desconectado no hay altas ni bajas (los mensajes siguen
    funcionando).
  - Si se pierde el committer, la membresía del grupo queda congelada.
  - Aún no se pueden añadir ni quitar *personas* de un grupo existente.
- **Pendiente:** transferir el rol de committer, administradores múltiples y
  resolución de bifurcaciones.

## D-008 · PeerId de libp2p = clave del dispositivo

- **Decisión:** el nodo libp2p usa como identidad la misma clave Ed25519 del
  dispositivo.
- **Motivo:** el `PeerId` se deduce directamente del registro de identidad,
  sin directorios de direcciones aparte, y el canal Noise queda autenticado con
  la misma clave.
- **Contra:** se usa una clave para dos fines (handshake Noise y firmas MLS).
  Ambos protocolos separan dominios, pero conviene revisarlo antes de la v1.
  La alternativa es una clave de red distinta firmada por la del dispositivo.

## D-009 · Transporte y entrega

- **Decisión:**
  - libp2p con TCP y QUIC, Noise y yamux.
  - mDNS para descubrir nodos en la red local, e identify.
  - Los mensajes van **directos de dispositivo a dispositivo** por
    request-response (CBOR), con confirmación.
- **Actualizado:** almacenamiento diferido en D-016, NAT en D-018 y
  encuentro entre redes en D-019.
- **Limitaciones de la PoC:** sin notificaciones push.
- **Pendiente:** en iOS, APNs con notificaciones vacías (no se puede evitar).

## D-010 · Orden y mensajes fuera de época

- **Decisión:**
  - Los envíos de cada dispositivo se serializan, para que un commit llegue
    antes que los mensajes de la nueva época.
  - Al recibir, un mensaje que no se puede procesar (llegó antes que su commit
    o antes que el Welcome) se guarda y se reintenta tras el siguiente commit,
    hasta 5 veces.
  - Se conservan las claves de 5 épocas pasadas.

## D-011 · Vinculación de dispositivos (PoC)

- **Decisión:** el dispositivo nuevo muestra su *código* (su clave pública en
  base32). En el primario, `link <código>` lo añade al registro, lo publica y
  le envía un `LinkGrant` con el registro firmado.
- **Riesgo:** un dispositivo sin identidad acepta el primer `LinkGrant` válido
  que lo incluya.
- **Pendiente:**
  - QR bidireccional con secreto de un solo uso y confirmación visual en ambos
    lados.
  - Transferir el historial entre dispositivos propios: ahora un dispositivo
    nuevo solo ve los mensajes posteriores a su entrada.

## D-012 · Almacenamiento local (PoC)

- **Decisión:** las claves y el estado se guardan en un directorio de datos
  con permisos `0600`. El estado MLS va en JSON (persistencia de
  `openmls_memory_storage`) y se escribe de forma atómica.
- **Limitación:** los mensajes no se guardan; solo se muestran.
- **Pendiente:** SQLite/SQLCipher y claves protegidas por el almacén seguro del
  SO (Keystore, Secure Enclave, TPM/DPAPI, Secret Service).

## D-013 · Una sola versión de ed25519-dalek

- **Decisión:** se fija `ed25519-dalek` 2.x y `rand` 0.8, las mismas que usa
  openmls 0.9, aunque existe ed25519-dalek 3.
- **Motivo:** evitar dos implementaciones de la misma criptografía y los
  conflictos de `rand_core`.
- **Excepción (D-019):** el crate `mainline` trae internamente
  ed25519-dalek 3 (prerelease). Solo lo usa para registros BEP 44, que no
  usamos; nuestra criptografía sigue en la 2.x.

---

## D-014 · Licencia GPL-3.0-or-later

- **Decisión:** el proyecto se publica bajo **GPL-3.0-or-later** (archivo
  `LICENSE`, campo `license` del workspace). Decisión del usuario, 2026-10-05.
- **Motivo:** copyleft. Quien distribuya una versión modificada debe publicar
  su código, algo importante en una aplicación de privacidad.
- **Compatibilidad comprobada:** las dependencias son MIT, Apache-2.0, BSD,
  ISC, Zlib, Unicode y MPL-2.0, todas compatibles con GPL-3.0.
- **A vigilar:**
  - No introducir dependencias GPL-2.0-only ni propietarias.
  - Si se usa `libsignal` (AGPL-3.0), la combinación pasaría a ser AGPL.
  - En iOS, la App Store tiene fricciones conocidas con GPL. Al ser los autores
    los dueños del copyright, se puede publicar igualmente, pero
    contribuciones de terceros sin acuerdo de licencia (CLA) complicarían esa
    vía. Conviene decidir pronto si se pide CLA o DCO.

## D-015 · Interfaz con Flutter + flutter_rust_bridge

- **Decisión:** las apps (Android, iOS, Linux, Windows, macOS) se hacen con
  Flutter (Dart). El núcleo Rust se expone con `flutter_rust_bridge`.
  Decisión del usuario, 2026-10-05.
- **Criterio de diseño:** la capa Flutter debe ser **fina y fácil de leer**:
  solo pantallas y estado de la interfaz. Toda la lógica (identidad, MLS, red,
  sincronización) queda en Rust, detrás de una API pequeña y documentada (un
  futuro crate `p2m-api`). La lógica que hoy está en `p2m-node/src/app.rs`
  pasará a ese crate para que la usen tanto la CLI como la app.

## D-016 · Almacenamiento diferido en buzones de otros peers

- **Decisión:** si un dispositivo no responde, el mensaje se mete en un
  **sobre** y se deja en buzones:
  - en el propio nodo del remitente;
  - y en hasta 3 nodos de la DHT cercanos a `mailbox_key(dispositivo)`.
  El destinatario vacía los buzones al conectar con un peer y en cada tick,
  confirma lo recibido (`AckEnvelopes`) y descarta duplicados.
- **Sobre:** sellado para el dispositivo destinatario (X25519 efímero contra
  su clave Ed25519 convertida a Montgomery, BLAKE3 y ChaCha20-Poly1305) y
  firmado por el dispositivo remitente. El buzón comprueba la firma pero no
  puede leer nada.
- **Solo se pide el buzón propio:** `Fetch` usa el PeerId autenticado por
  Noise, así que nadie puede vaciar el buzón de otro.
- **Límites contra abusos:** 500 sobres por destinatario, 2000 destinatarios,
  14 días y 256 KiB por sobre.
- **Motivo:** responde a la pregunta abierta. Confiar en otros peers no exige
  a cada usuario un nodo propio, y el cifrado hace que solo vean metadatos.
- **Riesgos:**
  - El buzón ve qué dispositivo escribe a cuál, cuándo y cuánto.
  - Reutilizar la clave Ed25519 como X25519 es habitual (libsodium lo
    permite) pero suma otro uso a la misma clave (ver D-008).
  - Un buzón puede tirar sobres: por eso hay varias copias.

## D-017 · KeyPackages *last resort* en la DHT

- **Decisión:** cada dispositivo publica en `/p2m/v1/kp/<dispositivo>` un
  KeyPackage reutilizable (extensión *last resort* de MLS), renovado cada
  7 días. Los nodos de la DHT lo validan antes de guardarlo.
- **Motivo:** el committer puede añadir a un chat a un dispositivo
  desconectado; el Welcome le llega por los buzones.
- **Contra:** reutilizar un KeyPackage reduce el secreto hacia adelante del
  primer mensaje hasta la siguiente actualización. Solo se usa si no hay uno
  fresco pedido directamente.

## D-018 · NAT: circuit relay v2, DCUtR y UPnP

- **Decisión:** todos los nodos son clientes de relay. Los que se arrancan con
  `--relay-server` ofrecen relay (límites generosos: 512 reservas, 1 h). Un
  nodo reserva hueco automáticamente en hasta 2 relays a los que ha llamado
  directamente. DCUtR intenta pasar a conexión directa y UPnP abre el puerto
  si el router lo permite.
- **Motivo:** sin esto, dos dispositivos tras NAT no pueden hablar.
- **Riesgo:** el relay ve qué PeerIds se conectan y cuánto tráfico pasa, pero
  no el contenido (Noise y MLS).
- **Corrección (2026-10-05):** el relay de libp2p solo anuncia el protocolo
  *hop* si el nodo tiene alguna dirección externa confirmada. Sin AutoNAT
  nadie la confirmaba, así que ningún `--relay-server` llegaba a ofrecer
  relay. Ahora un `--relay-server` da por externas sus direcciones TCP/QUIC
  de escucha, que es lo correcto en un servidor con IP pública. Pendiente:
  AutoNAT (D-019). Con esto el escenario de NAT del e2e pasa.

## D-019 · Encuentro entre redes: DHT Mainline de BitTorrent y trackers

- **Problema:** mDNS solo sirve en la red local. Un nodo de otra red necesita
  conocer al menos una dirección alcanzable.
- **Decisión:** los nodos con `--relay-server` se anuncian en un *infohash*
  fijo (`blake3("p2message/v1 rendezvous relays")`, 20 bytes):
  - en la **DHT Mainline** (BEP 5, crate `mainline`), cada 15 min;
  - y como respaldo en **trackers UDP públicos** (BEP 15), como *seeders*.
  Un nodo con menos de 3 conexiones pregunta por ese infohash cada minuto y
  llama a hasta 8 de los relays encontrados. Al conectar entra en nuestra DHT
  y, si está tras NAT, reserva hueco en el relay (D-018).
- **Motivo:** Mainline tiene millones de nodos y ningún dueño. Sus nodos de
  arranque llevan más de 15 años funcionando, así que no hace falta una lista
  fija de servidores propios. Decisión del usuario, 2026-10-05.
- **Las listas cambian con el tiempo:** se pueden sustituir sin recompilar
  con `--dht-bootstrap`, `--tracker` o un archivo `trackers.txt` en el
  directorio de datos. `--no-rendezvous` lo desactiva todo.
- **Seguridad:** solo se intercambian IP y puerto. La identidad la comprueba
  Noise al conectar, así que una entrada falsa solo cuesta un intento de
  conexión.
- **Privacidad:** los nodos de Mainline y los trackers ven que una IP usa
  p2message, pero no quién es: el infohash es el mismo para todos.
- **Limitaciones:**
  - Encontrar no es conectar: sigue haciendo falta al menos un relay
    alcanzable (IP pública o puerto abierto).
  - Un `--relay-server` tras NAT anuncia una dirección inalcanzable. Pendiente:
    anunciarse solo tras confirmar con AutoNAT que se es alcanzable.
  - Solo IPv4.
- **CGNAT:** muchas conexiones domésticas y casi todas las móviles tienen
  CGNAT. Ahí un nodo funciona bien como cliente (conexiones salientes al
  relay), igual que BitTorrent, pero no puede ser relay. Los móviles serán
  siempre clientes: dependen de relays con IP pública y de los buzones
  (D-016). DCUtR falla a menudo tras CGNAT y entonces el tráfico sigue por
  el relay.
- **Pendiente:**
  - Soporte IPv6: en móvil suele haber IPv6 sin traducción de direcciones,
    lo que facilita el hole punching.
  - ~~Direcciones del dispositivo en el QR y caché de peers en disco~~ →
    hecho en D-021.
  - ~~Modo `--headless` para desplegar relays~~ → hecho: `--headless` y
    `deploy/` (unidad systemd y Dockerfile).

## D-020 · Transporte sobre Tor (servicios onion)

- **Problema:** con CGNAT (casi todas las conexiones móviles y muchas
  domésticas), ningún nodo propio puede ser relay, y no queremos servidores
  públicos propios (D-019).
- **Decisión:** cada nodo de escritorio publica un **servicio onion v3** con
  Arti (`arti-client` 0.47, Tor en Rust y en proceso) y acepta conexiones
  libp2p por él. Es un `Transport` propio (`crates/p2m-node/src/tor.rs`) para
  direcciones `/onion3/<dirección>:4001`. Encima van Noise y yamux, como sobre
  TCP. Un servicio onion es alcanzable tras cualquier NAT, porque solo hace
  conexiones salientes.
  - No se usa `libp2p-community-tor`: está parado en libp2p 0.53 y Arti 0.24.
  - La clave del servicio onion la genera Arti y se guarda en `<datos>/tor`.
    **No** es la clave del dispositivo, para no reutilizar claves (D-008).
  - Puerto virtual 4001. No abre nada en la máquina; otros puertos cierran
    el circuito.
  - Activado por defecto; `--no-tor` lo desactiva (lo usan los tests e2e).
  - Las direcciones onion se difunden por identify y Kademlia como cualquier
    otra.
  - Tiempo de conexión de libp2p subido de 10 s a 60 s: un circuito hasta un
    servicio onion tarda bastante más. `--bootstrap` se reintenta cada 30 s
    mientras no haya conexiones, porque al arrancar Tor aún no está listo.
  - Arti con `static-sqlite`, para no depender de libsqlite3 del sistema
    (también necesario en Android).
- **Probado:** dos nodos tras CGNAT con `--no-listen --no-mdns
  --no-rendezvous`, es decir, solo Tor. Se conectan en unos 6 s una vez
  publicado el servicio (unos 20–40 s tras arrancar; la primera vez más,
  porque se descarga el directorio). `init`, `whois`, `chat` y mensajes en
  los dos sentidos funcionan.
- **Consecuencias:**
  - Los buzones (D-016) de cualquier nodo de escritorio encendido son
    alcanzables sin servidores públicos, así que los móviles pueden recoger
    mensajes a través de ellos.
  - Latencia de cientos de ms y algo más de batería: no importa para
    mensajería.
  - **No es un modo anónimo:** el nodo sigue anunciando también sus
    direcciones TCP, QUIC y mDNS. Un modo «solo Tor» sería ocultarlas (con
    `--no-listen --no-mdns --no-rendezvous`, más o menos lo que se probó).
  - Sigue haciendo falta conocer una primera dirección, aunque ahora puede
    ser una onion: el QR de un contacto, la caché de peers, o un nodo onion
    conocido de la comunidad. Este punto **sustituye a los relays con IP
    pública como vía principal** entre redes. Mainline y los trackers (D-019)
    se mantienen para quien sí tenga IP pública.
- **Pendiente:**
  - ~~Direcciones onion en el QR y caché de peers en disco~~ → hecho en D-021.
  - En móvil, Tor solo mientras la app está en primer plano (batería).
  - Sondear servicios onion propios desde fuera para verificar que siguen
    publicados.

## D-021 · Direcciones en el QR y caché de peers

- **Problema:** con Tor (D-020) cualquiera es alcanzable, pero hay que
  conocer una primera dirección.
- **Decisión:**
  - **QR:** pasa a `p2m:handle:clave[@dirección…]`. Las direcciones son
    multiaddrs completas (`…/p2p/<peer>`) del dispositivo que lo muestra,
    solo las alcanzables desde otra red: servicios onion e IPv4 públicas, sin
    privadas, CGNAT ni relays. Al crear un chat con un QR se llama primero a
    esas direcciones (hasta 60 s) y después se resuelve el handle. Las
    direcciones que no se entienden se ignoran, así que versiones futuras
    pueden añadir otros tipos. Los QR antiguos, sin `@`, siguen valiendo.
  - **Caché de peers:** `<datos>/peers.txt`, una dirección por línea, de
    hasta 64 peers con hasta 4 direcciones cada uno, del más reciente al más
    antiguo. Se rellena con lo que anuncian por identify los peers que hablan
    nuestro Kademlia, con el mismo filtro que el QR. Al arrancar se llama a
    estas direcciones y a las de `--bootstrap`, y se repite cada 30 s
    mientras no haya ninguna conexión.
  - Al conectar con un peer al que hemos llamado, su dirección entra en
    Kademlia sin esperar a identify. Si no, la primera búsqueda tras escanear
    un QR no tenía a quién preguntar.
- **Probado (solo Tor, tras CGNAT):**
  - Dos nodos nuevos, sin bootstrap: uno escanea el QR del otro, crea el chat
    y se envían mensajes.
  - Se reinicia uno sin bootstrap y reconecta solo, gracias a `peers.txt`.
- **Con esto el arranque no necesita ningún servidor:** el primer contacto
  llega por QR y, después, por la caché. Añadir solo por handle (sin QR)
  requiere estar ya en la red, sea por la caché, por un contacto anterior o
  por un relay de D-019.
- **Privacidad:** el QR revela la dirección onion del dispositivo y, si la
  tiene, su IP pública. Es razonable para alguien a quien le enseñas el QR en
  persona. Si en el futuro hay un modo anónimo, el QR solo llevará la onion.
- **Limitación observada:** el tiempo hasta publicar el servicio onion varía
  mucho. Con el directorio de Tor vacío se ha visto de 18 s a 7 min; en
  arranques posteriores, de 20 a 40 s. En móvil habrá que mostrar el estado
  («conectando a Tor…»).

## D-022 · Arquitectura de la app: núcleo como biblioteca y flutter_rust_bridge
- **Decisión:** `p2m-node` pasa a ser biblioteca y binario. `node::start`
  arranca red, punto de encuentro y lógica de chats, igual para la CLI que
  para la app. La app Flutter (`app/`) llama al núcleo mediante
  flutter_rust_bridge 2.13 a través del crate `app/rust` (`p2m_api`), que es
  miembro del workspace.
- **Interfaz:** el núcleo ya no imprime nada. Emite `Event` (vinculado, chat
  nuevo, mensaje, expulsión, miembro sin verificar, relay listo) por un canal.
  La CLI los imprime con el formato de siempre (del que depende `e2e.sh`). La
  app los recibe como `Stream<UiEvent>`.
- **Proceso y runtime:** hay un solo nodo por proceso, en un runtime de
  Tokio propio de `p2m_api`. Las funciones asíncronas de frb se ejecutan en
  ese runtime (`RT.spawn`). Las consultas locales (perfil, chats, mensajes,
  dispositivos) son síncronas.
- **Historial:** se guarda en `history.cbor` (0600), con hasta 5000 mensajes
  por chat, y se reescribe en cada mensaje. Está en claro en disco, como el
  resto de claves del directorio de datos. Pendiente: pasar a una base de
  datos (SQLite) cifrada con una clave del almacén del sistema (Keystore /
  Keychain / Secret Service).
- **Datos de la app:** `getApplicationSupportDirectory()/node`. La variable
  `P2M_DATA` permite abrir varias instancias en el mismo equipo para probar.
- **UI:** Material 3 con estas pantallas: alta (identidad nueva o vincular
  con código y QR), lista de chats con estado de red, chat, nuevo chat
  (handles o códigos `p2m:`, con título opcional para grupos) y perfil (QR,
  estado de Tor y relay, dispositivos). El escáner de QR con cámara queda
  para la versión móvil.

## Preguntas abiertas
- **Contribuciones:** ¿CLA o DCO? Afecta a la vía de la App Store (D-014).
