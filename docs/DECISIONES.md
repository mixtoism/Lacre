# Registro de decisiones

Cada decisión lleva su motivo y lo que queda pendiente. Si una decisión se
revisa, no se borra: se marca como **sustituida por D-xxx**.

Estado: endurecida tras una revisión de seguridad interna (D-039 a D-048),
2026-11-26. **No ha pasado ninguna auditoría externa**: hasta entonces no se
puede afirmar que sea segura.

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
  - ~~Aún no se pueden añadir ni quitar *personas* de un grupo existente.~~
    Superada por D-024.
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

> **Sustituida en parte por D-039:** el `LinkGrant` solo se acepta con el
> secreto de un solo uso del código de vincular.

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
- **Pendiente:** ~~SQLite/SQLCipher y claves protegidas por el almacén seguro del
  SO (Keystore, Secure Enclave, TPM/DPAPI, Secret Service).~~ *(Resuelto en
  D-033, sin SQLite: cada archivo de estado va cifrado.)*

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
  ~~firmado por el dispositivo remitente. El buzón comprueba la firma pero no
  puede leer nada.~~ *(Cambiado en D-034: remitente y firma van dentro del
  sello; el buzón solo ve el destinatario.)*
- **Solo se pide el buzón propio:** `Fetch` usa el PeerId autenticado por
  Noise, así que nadie puede vaciar el buzón de otro.
- **Límites contra abusos:** 500 sobres por destinatario, 2000 destinatarios,
  14 días y 256 KiB por sobre.
- **Motivo:** responde a la pregunta abierta. Confiar en otros peers no exige
  a cada usuario un nodo propio, y el cifrado hace que solo vean metadatos.
- **Riesgos:**
  - ~~El buzón ve qué dispositivo escribe a cuál, cuándo y cuánto.~~
    *(Resuelto en D-034: remitente sellado y depósito con identidad de un
    solo uso. Sigue viendo para quién, cuándo y cuánto.)*
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
  - ~~En móvil, Tor solo mientras la app está en primer plano (batería).~~
    *(Resuelto en D-034: Tor siempre, dormido en segundo plano.)*
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
  por chat, y se reescribe en cada mensaje. ~~Está en claro en disco, como el
  resto de claves del directorio de datos. Pendiente: pasar a una base de
  datos (SQLite) cifrada con una clave del almacén del sistema (Keystore /
  Keychain / Secret Service).~~ *(Resuelto en D-033: va cifrado.)*
- **Datos de la app:** `getApplicationSupportDirectory()/node`. La variable
  `P2M_DATA` permite abrir varias instancias en el mismo equipo para probar.
- **UI:** Material 3 con estas pantallas: alta (identidad nueva o vincular
  con código y QR), lista de chats con estado de red, chat, nuevo chat
  (handles o códigos `p2m:`, con título opcional para grupos) y perfil (QR,
  estado de Tor y relay, dispositivos). El escáner de QR con cámara queda
  para la versión móvil.

## D-023 · App Android
- **Compilación:** Gradle 9.3 con AGP 9.1 y el NDK r28 que descarga Gradle.
  Cargokit compila `p2m_api` para arm64 (y el resto de ABIs en release).
  Flutter usa el JDK de `flutter config --jdk-dir`, porque el `java` del
  PATH puede ser solo un JRE.
- **Contexto Android en Rust:** hickory-resolver (DNS de libp2p) lee la
  configuración del sistema mediante `ndk-context`. Por eso `MainActivity`
  carga `libp2m_api.so` y llama a `initAndroidContext(applicationContext)`
  (JNI, en `app/rust/src/android.rs`) antes de arrancar Flutter. Sin ese paso
  el nodo hace panic al arrancar.
- **Red:** permisos INTERNET, ACCESS_NETWORK_STATE y ACCESS_WIFI_STATE, más
  CHANGE_WIFI_MULTICAST_STATE con un `MulticastLock` mientras la actividad
  vive, para que funcione mDNS en la LAN.
- **DNS:** libp2p usa la configuración DNS del sistema, que en Android se
  lee por JNI de la red activa. Si en ese momento no hay red activa (app
  lanzada en segundo plano, cambio de red), la lectura falla; antes eso
  tumbaba el nodo entero. Ahora se usa Quad9 como respaldo (`net::dns_config`).
  Solo afecta a direcciones `/dns` (arranque), no a los mensajes.
- **Logs:** en Android, tracing emite registros de `log`, que van a logcat
  con la etiqueta `p2m`. Instalamos el logger antes que frb (gana el primero)
  para poder filtrar: `info,p2m_node=debug,libp2p_mdns=off`. yamux y rustls
  en Debug/Trace saturaban el búfer de logcat, y mDNS escribe un error por
  paquete en las interfaces de datos móviles (sin multicast), que borraba el
  resto del log. Para verlo: `adb logcat -s p2m:V`.
- **Errores de arranque:** si el nodo no arranca, la app muestra el error en
  lugar de quedarse en la pantalla de carga.
- **Pendiente:** ~~el nodo solo vive con la app en primer plano (no hay
  servicio en segundo plano ni notificaciones push, porque no hay servidores).~~
  *(Resuelto en D-032.)* Además: ~~Tor en el móvil (coste de batería) y no lanzar mDNS en interfaces móviles (ahora solo se
  silencian sus errores en el log).~~ *(Resuelto en D-034: modo de reposo;
  mDNS solo con wifi y en primer plano.)*
- **Prueba móvil ↔ PC:** `scripts/e2e-movil.sh [dispositivo]` arranca en el
  PC tres nodos solo por Tor y sin mDNS, y lanza en el móvil
  `app/integration_test/movil_test.dart`. Este cubre alta, chat 1:1 iniciado
  desde el móvil, grupo, chat iniciado desde el PC con el código del móvil,
  buzones con un nodo apagado y vinculación de un segundo dispositivo. Se
  coordinan por líneas `P2M_E2E …` en la salida del test. Funciona con el
  móvil por datos móviles (otra red, tras CGNAT). Ojo: el test instala su
  propia versión de la app; después hay que reinstalar la normal.
- **Prueba visible de grupo:** `scripts/e2e-grupo.sh [dispositivo]` abre
  una ventana de la app de escritorio por usuario del PC (ana, bea, carl) y
  la app en el móvil, con `app/integration_test/grupo_test.dart` compilado
  como app; se ve la interfaz mientras ocurre. El móvil abre el grupo con
  los códigos de los PC y todos comprueban que reciben los mensajes de
  todos. Después hay que recompilar e instalar la app normal.
- **QR entre escritorio y móvil:** da igual dónde se creara la cuenta: el
  escritorio muestra su QR (Perfil) y el móvil lo escanea. En Android e iOS
  hay un botón «Escanear QR» (mobile_scanner, permiso de cámara) en
  «Nuevo chat», que añade el código `p2m:` a los participantes, y en
  «Vincular dispositivo», que rellena el código del dispositivo nuevo. En
  escritorio no hay escáner: se pega el código. El perfil regenera su QR
  cuando cambia la dirección onion.

## D-024 · Añadir y quitar personas de un grupo

- **Decisión:** solo el committer (D-007) añade o quita personas. La lista de
  participantes nueva viaja **dentro del grupo**, como mensaje MLS de
  aplicación del committer, y después va el commit que añade o expulsa los
  dispositivos.
- **Motivo:** no se necesita ningún mecanismo nuevo de confianza. MLS ya
  autentica al remitente, y el resto solo acepta la lista si la firma el
  committer del chat, que no puede cambiar.
- **Cómo funciona:**
  - **Añadir:** el committer resuelve a la persona (handle o QR), envía la
    lista nueva a los miembros actuales y sincroniza. La sincronización añade
    sus dispositivos y les manda el Welcome con la lista.
  - **Quitar:** el committer envía la lista sin esa persona (el aviso también
    le llega a ella) y sincroniza. La sincronización expulsa los dispositivos
    de cualquier persona que ya no participa.
  - **Expulsar con un registro sin resolver:** a un dispositivo cuya persona
    no se ha resuelto (fallo de red) solo se le mantiene si el sufijo de su
    clave raíz coincide con el de algún participante. En ningún otro caso se
    mantiene.
  - **Volver a entrar:** quien ha salido puede volver a ser añadido. Su
    Welcome sustituye al grupo antiguo, con el mismo id, solo si viene del
    mismo committer (`replace_old_group` de openmls). El chat conserva su
    posición y su historial.
  - **Avisos:** todos los dispositivos calculan, a partir de la diferencia de
    listas, el aviso «ana#… añadió a carl#…» o «ana#… quitó a bea#…». El aviso
    se guarda en el historial (`notice`) y la app lo muestra centrado, sin
    burbuja.
  - **Interfaz:**
    - CLI: `members`, `add` y `remove`.
    - App: pantalla «Miembros» del chat, con «Añadir» (con escáner de QR en el
      móvil) y «Quitar».
- **Limitaciones:**
  - Si el committer cae entre enviar la lista y hacer el commit, los miembros
    pueden quedar con una lista desfasada hasta el siguiente cambio. Los
    dispositivos sí se ajustan en la siguiente sincronización.
  - ~~Nadie puede salir de un grupo por su cuenta: hay que pedírselo al
    committer.~~ *(Resuelto en D-031: borrar el grupo es salir.)*
  - Si se rechaza un Welcome (por ejemplo, de otro committer), el KeyPackage
    de un solo uso se pierde igualmente.
- **Pruebas:**
  - `scripts/e2e.sh`: quitar a Bob (con dos dispositivos), comprobar que no
    recibe y añadirlo de nuevo.
  - `scripts/e2e-grupo.sh`: el móvil quita al último usuario del PC y lo
    vuelve a añadir, con la interfaz visible.

## D-025 · Emojis y stickers, sin servicios externos
- **Decisión:**
  - Panel de emojis local en la app (`emoji_picker_flutter`).
  - Stickers propios, que viajan dentro del mensaje MLS cifrado como un
    mensaje más.
  - Sin GIPHY, Tenor ni ningún otro buscador.
- **Por qué:**
  - Un buscador externo pide una clave de API y ve qué busca cada usuario y
    desde qué IP. Además, la imagen se descargaría de sus servidores. Eso
    choca con «privacidad para todos».
  - Mandar los bytes de la imagen dentro del mensaje no revela nada nuevo a
    nadie y funciona también por buzón.
- **Cómo funciona:**
  - **Formato:** `AppMessage.sticker` lleva los bytes de la imagen; en ese
    caso `text` va vacío.
  - **Tipos admitidos:** PNG, GIF, WebP o JPEG, identificados por sus
    primeros bytes. Se rechaza cualquier otro, incluido SVG.
  - **Límites:**
    - 200 KB (`sticker::MAX_STICKER`).
    - 1024 px por lado (`MAX_SIDE`), leído de la cabecera.
    - Se comprueban al enviar y al recibir.
    - El límite de lado evita las «bombas de descompresión»: imágenes de
      pocos KB que declaran 30000×30000 px y agotan la memoria al
      decodificarlas.
  - **Bytes en CBOR:** los campos binarios del protocolo (`Request::Mls`, el
    Welcome, el KeyPackage y `Envelope.sealed`/`sig`) se serializan ahora como
    cadenas de bytes CBOR (`serde_bytes`), no como listas de enteros.
    - Antes cada capa casi duplicaba el tamaño: un sticker de 70 KB ya no
      cabía en un sobre de buzón de 256 KB. Ahora cabe uno de 200 KB.
    - Los nodos nuevos leen los dos formatos; los antiguos no entienden el
      nuevo. No hay versiones publicadas, así que no importa.
  - **Almacenamiento:**
    - Cada imagen se guarda una sola vez en `media/<blake3>.<ext>`, sea
      enviada, recibida o de la colección.
    - El historial guarda solo ese nombre (`StoredMessage.sticker`), no los
      bytes.
    - La colección propia es la lista de nombres en `stickers.cbor`.
    - El nombre se valida antes de abrir el archivo, para no salir de `media/`.
  - **Interfaz:**
    - Botón 😊 junto al campo de texto. Abre un panel con dos pestañas:
      - Emoji: categorías, recientes y búsqueda.
      - Stickers: la colección.
    - «Añadir stickers» importa imágenes con `file_selector`.
      - Si una imagen estática se pasa de tamaño, se reduce a 512 px (o menos)
        y se guarda como PNG.
      - Las animadas que se pasan se rechazan.
    - Con pulsación larga (o clic derecho), un sticker del chat se guarda en la
      colección, y uno de la colección se quita.
  - **CLI:** `sticker <n> <archivo>`.
- **Riesgos:**
  - Decodificar imágenes recibidas expone a fallos de los decodificadores de
    Flutter/Skia. Se reduce limitando formatos, tamaño y lado, y
    decodificando al tamaño mostrado (`cacheWidth`).
  - El tamaño del mensaje delata que es un sticker. Ver R-008, análisis de
    tamaño y tiempo, en la rama `worktree-doc-riesgos`.
- **Pendiente:**
  - Paquetes de stickers compartibles.
  - Un paquete por defecto incluido en la app.
  - Convertir GIF a WebP animado para que quepan más.
- **Pruebas:**
  - Unitarias en `sticker.rs`: formatos, dimensiones de cada formato, bomba
    rechazada y rutas.
  - `scripts/e2e.sh`:
    - Sticker directo.
    - Uno de 190 KB por buzón, con el destinatario desconectado.
    - Uno de 300 KB, rechazado.
  - `scripts/e2e-grupo.sh`, visible:
    - El móvil manda un emoji desde el panel y su sticker.
    - Cada PC guarda el del móvil y manda el suyo.

## D-026 · Archivos: enjambre privado dentro del grupo
- **Decisión:**
  - Los archivos de hasta 100 MB (`files::MAX_FILE`) se mandan como una
    oferta dentro del mensaje MLS.
  - El contenido viaja aparte, cifrado y en trozos. Lo sirven los
    dispositivos del grupo que ya lo tienen, por nuestra propia red libp2p.
  - No se usa BitTorrent público.
- **Por qué no BitTorrent:**
  - En la DHT y en los trackers públicos cualquiera ve quién comparte qué
    infohash y desde qué IP. Aunque el contenido fuera cifrado, eso revela
    quién habla con quién y cuándo.
  - Nuestra red ya autentica a los dispositivos, va por Tor y sabe quién es
    miembro del grupo.
  - Del modelo de BitTorrent se conserva lo útil: trozos verificables y
    varias fuentes a la vez.
- **Cómo funciona:**
  - **Cifrado:**
    - Cada archivo tiene una clave aleatoria de 32 bytes.
    - Se parte en trozos de 256 KiB.
    - Cada trozo se cifra con ChaCha20-Poly1305, usando su índice como
      nonce. La clave es única por archivo, así que el cifrado es
      determinista y quien lo tenga sirve exactamente los mismos bytes.
  - **Oferta** (`FileOffer`, dentro del `AppMessage` cifrado):
    - Nombre, tamaño y clave.
    - El BLAKE3 de cada trozo cifrado, para verificar cada trozo antes de
      descifrarlo, venga de quien venga.
  - **Identificadores derivados de la clave:**
    - Id del archivo: `derive_key("p2message 2026 file id", clave)`.
    - Token de petición: `derive_key("p2message 2026 file token", clave)`.
  - **Petición:** `Request::Chunk {file, index, token}` por `/p2m/msg/1`.
  - **Quién recibe un trozo:** quien sirve solo lo entrega si se cumplen las
    tres condiciones:
    - El token es correcto (comparación en tiempo constante).
    - El dispositivo que pide es miembro actual del grupo MLS de la oferta.
    - Quien sirve tiene el archivo completo.
  - **Descarga:**
    - Cuatro peticiones en paralelo.
    - Primero se pide al remitente y luego a los demás miembros.
    - Una fuente que falla se descarta durante esa descarga.
    - Se guarda en `files/<id>/<nombre>.part` y se renombra al terminar.
    - Para reanudar, se vuelven a cifrar los trozos ya escritos y se comparan
      sus hashes, sin un mapa aparte.
  - **Envío:** el archivo se cifra una vez para calcular los hashes y se copia
    a `files/<id>/<nombre>`, desde donde se sirve.
  - **Nombre:** se limpia al enviar y se comprueba al recibir
    (`safe_name`), para no salir de `files/<id>/`.
  - **Interfaz:**
    - Botón 📎 en el chat.
    - Burbuja con nombre y tamaño, y botones «Descargar» (con progreso,
      `UiEvent::FileProgress`) y «Abrir».
    - En Android se abre con otra app mediante un `FileProvider`.
    - La descarga es manual: no se baja nada sin que la persona lo pida.
  - **CLI:** `file <n> <archivo>` y `get <id>`.
- **Límites:**
  - Solo se descarga de dispositivos en línea. Los buzones no guardan
    archivos: con 100 MB por archivo se llenarían enseguida. Si nadie que lo
    tenga está conectado, hay que probar más tarde. Se puede añadir más
    adelante, por ejemplo con buzones que guarden trozos cifrados un tiempo.
  - Una persona quitada del grupo conserva lo que ya descargó y la clave de
    lo que se mandó mientras estaba. Ya no puede pedir trozos, porque deja de
    ser miembro.
  - En Android, `file_selector` lee el archivo entero en memoria al elegirlo
    (hasta 100 MB) y deja una copia en la caché. La app borra esa copia al
    enviar.
  - El tamaño y el momento de las descargas se ven en la red. Ver R-008 en
    la rama `worktree-doc-riesgos`.
- **Pruebas:**
  - Unitarias en `files.rs`:
    - Nombres y límites.
    - Enviar, servir y descargar, incluidos un token incorrecto, un trozo
      corrupto y la reanudación de una descarga a medias.
  - `scripts/e2e.sh`:
    - Archivo de 3 MB enviado y descargado, comprobado con `cmp`.
    - Con el remitente desconectado, otro miembro lo sirve a un tercero.
    - Uno de 101 MB, rechazado.
  - `scripts/e2e-grupo.sh`, visible: el móvil manda 1 MB y cada PC lo
    descarga desde la interfaz y comprueba el contenido.

## D-027 · Un solo chat 1:1 entre dos personas
- **Problema:** se podían tener varios chats 1:1 entre las mismas dos
  cuentas. Pasaba al crearlo dos veces o cuando los dos lo creaban a la vez.
  Los mensajes quedaban repartidos entre chats.
- **Decisión:** los chats 1:1 duplicados se funden en uno al mostrarlos, sin
  tocar los grupos MLS ni lo guardado.
  - Un chat es 1:1 si está activo, no tiene título y tiene 2 participantes.
    Su clave son los dos sufijos de handle, ordenados.
  - El chat canónico es el de menor índice con esa clave. Los demás no salen
    en la lista.
  - El historial junta el de todos sus grupos, ordenado por fecha.
  - Los mensajes que llegan por cualquiera de los grupos se notifican en el
    canónico, y los envíos van por el canónico.
  - Crear un chat 1:1 que ya existe devuelve el que hay. Si mientras se crea
    llega la invitación del otro, se devuelve el canónico.
- **Por qué así:** no hace falta coordinar a los dos lados ni cambiar el
  formato guardado. El canónico se calcula al vuelo y ambos lados ven un solo
  chat aunque cada uno tenga un índice distinto.
- **Límite:** los grupos duplicados siguen existiendo por debajo. Un mensaje
  puede salir por un grupo y llegar por otro, pero se ve en el mismo chat.
- **Pruebas:** `scripts/e2e.sh` crea un 1:1 que ya existe y otro a la vez
  desde los dos lados, y comprueba que cada uno ve un solo chat.

## D-028 · Conexiones vivas con los miembros de los chats
- **Problema:** sobre Tor, lo lento es montar la conexión: buscar el
  descriptor del servicio onion y hacer el rendezvous, varios segundos. La
  red cierra las conexiones tras 120 s sin uso, así que casi cada mensaje
  tenía que montarla otra vez.
- **Decisión:**
  - Cada 45 s (`KEEPALIVE_EVERY`) se manda un `Request::Ping` a cada
    dispositivo de nuestros chats activos. Ese tráfico evita que se cierre la
    conexión y mantiene caliente el circuito.
  - El primer ping sale a los 3 s de arrancar, así que sirve también de
    preconexión con las direcciones conocidas (D-021).
  - Si un dispositivo no responde, se espera antes de volver a probar: de
    1 min, doblando hasta 15 min. Así no se gasta red en quien está
    desconectado.
  - Se registran tiempos (`ms`) del arranque de Tor, del servicio onion
    alcanzable, de cada llamada por Tor, de cada ping y de cada envío
    directo, para medir dónde se va el tiempo.
- **Coste:** un mensaje pequeño cada 45 s por dispositivo conectado, más
  batería en el móvil (en segundo plano ya no: modo de reposo, D-034). Con muchos contactos habrá que limitarlo, por ejemplo
  a los chats con actividad reciente.
- **Privacidad:** se ve que dos dispositivos hablan aunque no se manden
  mensajes. Sobre Tor solo se ve en los extremos. Entre mensajes ya se veía
  la conexión abierta hasta 120 s.

## D-029 · Solicitudes de chat, bloqueo, borrar chats y búsqueda
- **Problema:** cualquiera que conozca nuestro handle puede abrirnos un chat
  o meternos en un grupo. Hasta ahora salía como un chat más, con sus
  stickers a la vista y sus archivos listos para descargar. Faltaba también
  poder bloquear a alguien, borrar un chat y buscar en los mensajes.
- **Decisión (solicitudes):**
  - Un chat que nos llega es una **solicitud** si no viene de alguien de
    confianza. De confianza es: uno de nuestros dispositivos, o quien ya
    está en un chat aceptado con nosotros (un 1:1, o un grupo que creamos
    nosotros). Que coincidamos en un grupo creado por un tercero no cuenta.
  - Mientras es solicitud los mensajes llegan y se ven, pero no se descargan
    sus archivos (el núcleo se niega) y la app no muestra sus stickers.
    Tampoco le damos nuestra foto de perfil (D-030).
  - Se acepta con «Aceptar» o al contestar. Aceptar en un dispositivo lo
    acepta en todos los nuestros: se manda `Request::Accepted { groups }` a
    nuestros otros dispositivos (por buzón si no están). Al conectar con un
    dispositivo nuestro también se le manda la lista, para el que se acaba de
    vincular. Si la aceptación llega antes que la invitación, se guarda
    (`pre_accepted`) y se aplica al llegar.
  - `Chat.accepted` se guarda con valor por defecto `true`, así que los
    chats que ya existían siguen aceptados.
- **Decisión (bloqueo):**
  - Se bloquea por sufijo de handle, que sale de la clave raíz: vale aunque
    la persona cambie de nombre o de dispositivos.
  - Se descartan sus mensajes antes de guardarlos. Se ocultan sus chats 1:1
    y sus invitaciones a grupos sin aceptar. No se le pasa nuestra foto ni
    se descarga la suya.
  - En los grupos ya aceptados se ocultan sus mensajes; el grupo sigue.
  - No se le avisa. Al desbloquear vuelven a verse sus chats, pero sin los
    mensajes que mandó mientras estaba bloqueada.
- **Decisión (borrar):** borrar un chat lo oculta y vacía su historial en
  este dispositivo, pero no se sale del grupo MLS. Si llega otro mensaje,
  reaparece vacío. Salir del grupo de verdad necesita un commit del creador
  y queda para más adelante. *(Sustituido para grupos por D-031: borrar un
  grupo que no gestionamos es salir de él.)*
- **Decisión (búsqueda):**
  - Todo se busca en local, en el historial descifrado del dispositivo. No
    hay índice: se recorren los mensajes, lo que basta para los volúmenes de
    un chat personal.
  - No distingue mayúsculas ni tildes.
  - Busca en el texto y en los nombres de archivo. Da 200 resultados como
    máximo, del más reciente al más antiguo, con su posición para saltar al
    mensaje.
- **Limitaciones:**
  - La lista de bloqueados no se sincroniza entre nuestros dispositivos.
    *(Resuelto en D-031.)*
  - Los stickers de una solicitud ya están en el disco (vienen dentro del
    mensaje, D-025); solo no se muestran.
  - Un grupo de un tercero en el que coincidimos con un desconocido no le
    hace de confianza: si este nos abre un 1:1, llega como solicitud.

## D-030 · Fotos de perfil
- **Decisión:**
  - La foto se pide, no se reparte. Cada mensaje lleva la versión de la
    foto de quien escribe (`AppMessage.avatar`, una marca de tiempo; 0 si no
    tiene). Al cambiarla se manda un mensaje vacío a los chats aceptados,
    solo para anunciar la versión nueva; no se guarda en el historial.
  - Quien ve una versión que no tiene la pide con `Request::Avatar` al
    dispositivo que la anunció, y solo si esa persona está en un chat
    aceptado con él.
  - Ese dispositivo solo la da a dispositivos que comparten con él un chat
    activo y aceptado, y cuya persona no está bloqueada. Un desconocido no
    puede ver nuestra cara pidiéndonos un chat.
  - Formato: PNG, GIF, WebP o JPEG de 128 KB y 512 px de lado como máximo.
    La app la recorta en cuadrado y la reduce a 256 px (o menos) en PNG.
  - Se guardan en `avatars/<sufijo>-<versión>.<ext>`. Al llegar una versión
    nueva se borra la anterior.
- **Por qué no ir en cada mensaje:** 128 KB por mensaje sería demasiado. Ir
  solo en el anuncio también falla: quien esté desconectado o entre después
  en el grupo no la recibiría.
- **Privacidad:** quitar la foto es una versión más (sin imagen), así que
  también se propaga. Quien ya la descargó la conserva hasta que llega la
  versión nueva.

## D-031 · Salir de un grupo al borrarlo y bloqueos en todos los dispositivos
- **Contexto:** cierra dos limitaciones de D-029. Decidido sin preguntar
  (se pidió seguir sin parar y anotar aquí lo decidido).
- **Decisión (salir al borrar):**
  - Borrar un **grupo** que gestiona otro dispositivo es salir de él. Se
    manda al grupo un mensaje MLS con `leave` (va cifrado y autenticado: el
    committer sabe quién lo pide sin más firmas). El committer quita a esa
    persona (todos sus dispositivos) con el aviso «X salió del chat», en vez
    de «Y quitó a X».
  - Se sale como persona, no como dispositivo: un dispositivo suelto no
    puede estar en un grupo sin los demás de su persona (D-024). Nuestros
    otros dispositivos ven «Saliste del chat desde otro dispositivo»; no
    pierden el historial, pero el chat queda inactivo.
  - Desde que salimos (`Chat.left`) se ignora todo lo que llegue a ese
    grupo, aunque el commit que nos saca tarde: el chat no reaparece. Que
    nos saquen no se muestra como expulsión.
  - Si nos vuelven a añadir, llega como **solicitud** aunque antes lo
    hubiéramos aceptado: salimos a propósito.
  - Si el grupo lo gestiona este dispositivo, borrar solo lo oculta, como en
    D-029: el grupo sigue para los demás. Pasar la gestión a otro queda para
    más adelante. La app lo explica en la confirmación («Salir y borrar»
    frente a «Borrar»).
  - Un 1:1 sigue como en D-029: solo se oculta y reaparece con el siguiente
    mensaje.
  - Los avisos (alguien entra o sale) ya no hacen reaparecer un chat
    borrado; solo un mensaje.
- **Decisión (bloqueos sincronizados):**
  - Cada bloqueo o desbloqueo se guarda con su hora y se manda a nuestros
    otros dispositivos con `Request::Blocks` (por buzón si no están; al
    conectar se manda la lista entera). Solo se admite de un dispositivo de
    nuestro registro.
  - Gana el cambio más reciente por persona; los desbloqueos también se
    recuerdan, para que una lista vieja no vuelva a bloquear. Un cambio
    local siempre queda posterior al que había, aunque los relojes no
    coincidan.
  - La lista antigua (`blocked.cbor` como sufijo → handle) se lee igual.
  - Al vincular un dispositivo, el que lo vincula le manda los chats
    aceptados y los bloqueos justo después de darle la identidad: al
    conectar todavía no la tenía y rechazaba la lista, así que a veces
    veía como solicitud un chat ya aceptado.
- **Limitaciones:** si el committer está desconectado, la salida espera en
  su buzón y mientras tanto seguimos en el grupo MLS (aunque no se muestre
  nada). Si el committer desaparece para siempre no hay forma de salir del
  grupo, igual que no la hay de añadir a nadie.

## D-032 · Android en segundo plano y avisos de mensajes
- **Contexto:** sin servidores no hay notificaciones push (FCM pasaría por
  Google y revelaría cuándo nos escriben). Para recibir mensajes con la app
  cerrada, el nodo tiene que seguir vivo en el móvil. Decidido sin preguntar.
- **Decisión:**
  - El motor de Flutter (y con él el nodo) se crea en la `Application`
    (`P2mApp`), no en la actividad, y se guarda en `FlutterEngineCache`.
    `MainActivity` lo reutiliza y no lo destruye al cerrarse.
  - Al arrancar el nodo, Dart pide el servicio en primer plano `NodeService`
    (tipo `specialUse`, `START_STICKY`). Solo mantiene vivo el proceso. Su
    aviso fijo («Conectado para recibir mensajes», canal de prioridad
    mínima) tiene la acción «Cerrar», que termina el proceso.
  - Si Android mata el proceso, el servicio lo relanza: `P2mApp` crea el
    motor y Dart vuelve a arrancar el nodo con su clave (D-033).
  - Los avisos de mensajes los lanza Dart por el canal `p2message/sistema`,
    solo si la app no está en primer plano. Título: quién escribe (1:1) o el
    nombre del grupo. En una **solicitud** no se muestra el contenido, solo
    «Solicitud de chat de X». En la pantalla de bloqueo solo se ve «Mensaje
    nuevo» (`VISIBILITY_PRIVATE` con versión pública).
  - Tocar el aviso abre ese chat. Abrir un chat quita su aviso.
  - El permiso de notificaciones se pide al abrir (Android 13+). En el
    perfil, la app indica si está exenta de la optimización de batería y
    ofrece pedirlo; no lo pide sola.
  - `allowBackup=false`: el estado no va a las copias de Google.
- **Limitaciones:**
  - Con Doze y sin la exención de batería, la red se corta con la pantalla
    apagada y los mensajes llegan al despertar (o por buzón).
  - MIUI y otras capas matan servicios si no se permite el «Inicio
    automático». La app no puede activarlo; hay que hacerlo a mano.
  - ~~Gasto de batería: la conexión sigue abierta. Tor en el móvil sigue
    pendiente por eso (D-023).~~ *(Resuelto en D-034: modo de reposo.)*
  - En escritorio la app solo recibe mientras está abierta.

## D-033 · Cifrado del estado en disco

> **Sustituida en parte por D-042:** el formato pasa a v2 (`P2MVLT2`, con la
> ruta en el AAD). En el escritorio ya no se sigue sin cifrar si falta el
> llavero: hay llavero en Linux, macOS y Windows, y sin clave la app no
> arranca.
- **Contexto:** claves, historial, chats y estado MLS iban en claro con
  permisos `0600` (D-012). Con el dispositivo robado o una copia del disco se
  leía todo. Decidido sin preguntar.
- **Decisión:**
  - Sin SQLite/SQLCipher. Compilar SQLCipher para Android en cada
    arquitectura complica la build, y el estado ya son archivos pequeños.
    Cada archivo de estado se cifra entero (`p2m_core::vault`):
    XChaCha20-Poly1305 con nonce aleatorio de 24 bytes y el nombre del
    archivo como datos asociados (no se puede cambiar un archivo por otro).
    Cabecera `P2MVLT1\0`. Escritura atómica (temporal + `rename`, `fsync`,
    `0600`).
  - El estado MLS pasa de JSON a CBOR (más compacto). Se sigue leyendo el
    JSON antiguo.
  - Una sola clave de datos aleatoria de 32 bytes por directorio de datos:
    - **Android:** se guarda envuelta (AES-GCM) con una clave del Android
      Keystore que no sale del hardware, en `noBackupFilesDir`.
    - **Escritorio Linux:** en el llavero del sistema (Secret Service:
      GNOME Keyring, KWallet), una entrada por directorio de datos.
    - **CLI y pruebas:** variable `P2M_DATA_KEY` (64 hex). Si existe, tiene
      prioridad sobre el llavero; así las pruebas no lo llenan.
  - Al arrancar con clave, los archivos en claro se cifran (migración). Con
    el estado cifrado y sin clave, o con otra, el nodo no arranca: no se
    sobreescribe nada.
  - Si no hay llavero ni estado cifrado, el nodo arranca **sin cifrar** y el
    perfil lo avisa («Datos sin cifrar en el disco»). Mejor eso que no
    poder usar la app en un escritorio mínimo.
- **Limitaciones:**
  - Siguen en claro los stickers, las fotos de perfil, los archivos
    descargados y el estado de Tor (pendiente, hoja de ruta). *(Los
    primeros, cifrados desde D-042; la clave onion, fuera del disco desde
    D-044.)*
  - La clave se desbloquea con el dispositivo (Keystore) o con la sesión
    (llavero): protege el disco apagado o copiado, no un móvil desbloqueado.
  - Sin la clave (llavero borrado, Keystore restablecido), el estado no se
    puede recuperar. Hace falta la frase de recuperación (hoja de ruta).
  - macOS, Windows e iOS aún no tienen almacén de clave.

## D-034 · Remitente sellado, entrega activa desde los buzones y modo de reposo
- **Contexto:** se pidió que el remitente viaje dentro del mensaje firmado,
  para que los buzones no sepan quién habla con quién. También que un buzón
  que guarda algo para A#123 se lo lleve, buscando su dirección onion, y que
  Tor pueda estar siempre activo en el móvil sin un gasto de batería
  enorme. Los detalles se decidieron sin preguntar.
- **Remitente sellado:**
  - El sobre (`p2m_core::envelope`, v2) solo lleva fuera el **destinatario**
    y el texto sellado. Dentro va `{remitente, firma, contenido}`. La firma
    cubre destinatario, clave efímera y contenido, así que el contenido
    firmado no se puede reenvolver para otro. El destinatario descifra,
    comprueba la firma y procesa la petición como si viniera del remitente.
  - Id del sobre: `BLAKE3(sellado)`, para confirmar y descartar duplicados.
  - El buzón ya no puede comprobar firmas: solo que el destinatario es una
    clave válida y que el sobre no está truncado. Los límites (por
    destinatario, tamaño, 14 días) siguen. La firma del sobre tampoco
    frenaba el spam: cualquiera puede crearse una clave.
  - **Mensajero:** depositar por la conexión normal revelaría al buzón
    nuestro PeerId (Noise autentica la clave de quien llama). Por eso los
    depósitos van por un swarm de un solo uso, con una identidad libp2p
    nueva cada vez. Si hay Tor y el buzón tiene dirección onion, solo se usa
    esa, con un cliente Tor **aislado**: no comparte circuitos con los del
    nodo, y el buzón no ve ni la IP.
  - Solo si así no se guarda ninguna copia se deposita con la identidad
    propia. Mejor entregar que perder el mensaje. Ese caso queda en el log.
  - Los sobres antiguos (v1) no se pueden abrir: se confirman y se
    descartan. Un `mailbox.cbor` ilegible se ignora con un aviso.
- **Entrega activa desde los buzones:**
  - Un nodo que guarda sobres para un dispositivo se los lleva
    (`Deliver` → `Taken{ids}`). `Client::request` busca al destinatario en
    la DHT si no sabe llegar, así que encuentra su dirección onion.
  - Cuándo:
    - En cuanto el destinatario se conecta.
    - Si no, en el tick: primer intento al minuto, más un azar de hasta
      30 s para no delatar el momento del depósito.
    - Después a los 5 min, 30 min y cada 2 h, hasta que caduca el sobre.
  - Una ronda a la vez, y hasta 768 KiB por petición (las peticiones tienen
    un límite de 1 MiB).
  - El destinatario solo acepta sobres para él, descarta duplicados (los
    mismos ids que con `Fetch`) y admite hasta 300 sobres por peer cada
    10 min. También confirma los que no puede abrir, para que el buzón no
    insista con basura.
  - Sigue habiendo `Fetch`: si un buzón no llega, el destinatario los pide.
  - **Privacidad:** el buzón ya conocía la dirección onion del destinatario
    (se difunde por identify y la DHT). Llevarle los sobres le dice si está
    en línea, nada que no pudiera averiguar con un ping. Como no sabe quién
    escribió, no relaciona remitente y destinatario.
- **Modo de reposo** (`Power { low, wifi }`; API `set_power(background,
  wifi)`, CLI `power low|full [wifi|mobile]`):
  - En Android, Dart lo activa cuando la app pasa a segundo plano
    (`AppLifecycleListener`) y lo quita al volver. Al quitarlo hay un tick
    inmediato. Al arrancar sin pantalla (servicio relanzado), arranca en
    reposo. Kotlin avisa de los cambios de red (wifi o cable frente a datos)
    con un `NetworkCallback`.
  - En reposo:
    - **Solo se entra por el servicio onion.** Con el servicio publicado, se
      sueltan las reservas en relays y no se piden otras. TCP y QUIC siguen
      escuchando, pero sin keepalive (D-028) las conexiones se cierran
      solas a los 120 s.
    - **Tor dormido:** `TorClient::set_dormant(Soft)`. Arti para el relleno
      de los canales y sus tareas periódicas. El servicio onion sigue
      atendiendo, y Tor despierta al usarse.
    - **Relleno reducido** (`PaddingLevel::Reduced`) en Android, también en
      primer plano.
    - **Tareas agrupadas cada 12 min:** tick (publicar, vaciar y entregar
      buzones, sincronizar) y volver a llamar a los nodos de arranque. Sin
      keepalive, sin punto de encuentro, y la DHT Mainline se apaga.
    - **mDNS** solo en primer plano y con wifi. Sin mDNS se suelta el
      `MulticastLock`, que impedía al wifi filtrar paquetes.
    - Dart no consulta el estado del nodo cada 5 s.
- **Medición en el Pixel** (Pixel 8 Pro, sin exención de batería; series
  de muestras por minuto en `scripts/bateria/`: CPU del proceso en ticks de
  10 ms y bytes de `batterystats`). Las series son cortas (11 y 9 min) y el
  teléfono alternaba entre Doze y pantalla encendida, así que solo se
  comparan minutos en el mismo estado:
  - **En Doze profundo:** 0 ticks en las dos versiones. Android congela el
    proceso.
  - **Pantalla apagada, proceso activo** (ventanas de mantenimiento y
    después de usar el móvil):
    - Antes: 1 700–2 000 ticks por minuto, es decir, 17–20 s de CPU, sobre
      todo en los hilos de tokio.
    - Ahora: 170–735 ticks por minuto, unos 450 de media (4,5 s), en reposo
      y con Tor encendido. Es unas **4 veces menos de CPU**, y antes ni
      siquiera había Tor.
  - **Red:** con la pantalla apagada, entre 10 y 150 KB por minuto por
    wifi en las dos versiones. La medida es demasiado ruidosa para
    compararlas: arranque, reconexiones de Tor y descarga del directorio.
  - Para medirlo sin abrir la pantalla no sirve
    `am start-foreground-service`, porque el servicio no está exportado. Se
    abre la app y se vuelve al inicio: en el log aparece
    `reposo: Mainline detenida` y se suelta el `MulticastLock`.
  - Pendiente: una medición larga (horas) con
    `dumpsys batterystats --charged` para tener mAh reales.
- **Limitaciones:**
  - Los relays de circuito siguen viendo los PeerId de los dos extremos de
    un circuito. El mensajero no usa relays, solo TCP, QUIC u onion.
  - Sin Tor, el buzón ve la IP del mensajero (en una red local, la de
    siempre).
  - Kademlia mantiene su bootstrap cada 5 min e identify su intervalo:
    libp2p no permite cambiarlos en marcha.
  - En Doze profundo Android congela el proceso aunque tenga servicio en
    primer plano. Lo recibido llega en las ventanas de mantenimiento o, por
    buzón, al despertar. Con la exención de batería hay más ventanas.
  - Tras un reposo largo, el primer envío espera a que Tor despierte y
    renueve el directorio.
  - No es compatible con nodos anteriores: sobres v1 y v2 no se entienden.

## D-035 · Avisos en el escritorio y qué cuentan los avisos
- **Contexto:** la app de Linux no usaba los avisos del sistema. Además, un
  aviso con el remitente y el texto queda a la vista de cualquiera: en la
  pantalla, en la de bloqueo y en el historial de avisos del sistema (y en
  Android, de las apps con acceso a las notificaciones).
- **Decisión:**
  - **Linux:** avisos por `org.freedesktop.Notifications`
    (`flutter_local_notifications_linux`: solo la parte de Linux, que es
    Dart sobre D-Bus, para no meter código en Android). Como en Android,
    solo cuando la ventana no está a la vista: sin foco Flutter la deja en
    `inactive`. Al pulsar el aviso se abre el chat. Al abrir un chat se
    quita su aviso. Sin servidor de avisos se sigue sin ellos.
  - **Qué dicen** (ajuste por dispositivo, `avisos.txt` en el directorio de
    datos; no es secreto):
    - «Quién escribe y el mensaje».
    - «Solo quién escribe»: «Mensaje nuevo» o «Solicitud de chat».
    - «Solo "Tienes mensajes nuevos"»: un único aviso para todos los
      chats, que abre la app pero no un chat concreto.
  - Se pregunta la primera vez que se ve la lista de chats, también en
    instalaciones que ya existían, y se puede cambiar en Perfil. Mientras
    no se elige, los avisos no cuentan nada.
  - De las solicitudes de chat nunca se enseña el contenido, y en la
    pantalla de bloqueo de Android el aviso público sigue siendo «Mensaje
    nuevo» (D-032).
- **Limitaciones:**
  - En Wayland, al pulsar el aviso se abre el chat, pero la ventana no
    pasa al frente: Wayland no deja a una app robar el foco.
  - El ajuste es de cada dispositivo, no se sincroniza entre los de una
    misma identidad.

## D-036 · El proyecto pasa a llamarse Lacre
- **Contexto:** el nombre del proyecto pasa de p2message a Lacre. Cambiar
  los identificadores que viajan por la red cambiaría todos los handles y
  rompería la compatibilidad con los nodos que ya existen (PROTOCOLO.md §15).
- **Decisión:** se renombra todo menos la red.
  - **Cambia:**
    - El nombre visible: «Lacre» en la app, los avisos, la ventana y la
      documentación.
    - Los crates: `p2m-core` → `lacre-core` y `p2m-node` → `lacre-node`
      (también el binario de la CLI, `lacre-node`). `p2m_api` →
      `lacre_api`, y el módulo de FRB `api::p2m` → `api::lacre`.
    - El paquete de Dart: `p2message` → `lacre`.
    - El ID de la app: `es.p2message.p2message` → `org.lacre.app`, en
      Android y en Linux. En Android la clase `P2mApp` pasa a ser
      `LacreApp`.
    - El canal `lacre/sistema` y el alias del Keystore `lacre-datos`.
    - En el llavero, la etiqueta `application=lacre`.
    - Las variables de entorno `P2M_*` → `LACRE_*`.
    - La unidad de systemd: `deploy/lacre-node.service`.
  - **No cambia:**
    - Los protocolos libp2p `/p2m/kad/1.0.0`, `/p2m/msg/1` y `/p2m/id/1`.
    - Las claves de la DHT `/p2m/v1/…`.
    - Las cadenas de separación de dominio `p2message/…`.
    - Los contextos de clave de los archivos, `p2message 2026 file …`.
    - El prefijo `p2m:` del QR.
    - El apodo `p2message` del servicio onion. Si cambiara, el nodo
      tendría otra dirección .onion.
    - La cabecera `P2MVLT1` del estado cifrado en disco.
  - No se reescriben las entradas anteriores de este registro: reflejan
    los nombres de su momento.
  - El directorio del repositorio sigue siendo `p2message`, para no romper
    las sesiones abiertas.
- **Consecuencias:**
  - Android ve una app nueva (`org.lacre.app`), que convive con la
    anterior. La identidad de la app vieja no pasa a la nueva: hay que
    crear la identidad de nuevo (o añadir el dispositivo) y desinstalar
    `es.p2message.p2message`.
  - En Linux, los datos pasan a `~/.local/share/org.lacre.app` y el
    llavero busca `application=lacre`. Una instalación anterior empieza de
    cero.
  - Hay un proyecto de software libre anterior llamado Lacre (cifrado de
    correo con OpenPGP, de Disroot). Puede haber confusión o un conflicto
    de marca.
  - PROTOCOLO.md la mantiene la sesión del RFC. Aquí solo se han cambiado
    en ella los nombres de los crates.

## D-037 · Vincular en los dos sentidos y entre redes distintas
- **Contexto:**
  - Solo el dispositivo nuevo mostraba un código, y el primario tenía que
    escanearlo. Si el primario era el escritorio (sin cámara) y el nuevo un
    móvil, había que copiar el código a mano.
  - El código era solo la clave del dispositivo. Entre redes distintas el
    primario no sabía cómo llegar al nuevo.
  - Además, el primario añadía el dispositivo al registro *antes* de
    comprobar que llegaba a él. Si no llegaba, salía un error en crudo y el
    registro quedaba con un dispositivo que no sabía que estaba vinculado.
- **Decisión:**
  - **Quien no tiene cámara muestra el QR**, sea primario o nuevo:
    - El dispositivo nuevo muestra su código, como hasta ahora.
    - El primario muestra además una **invitación**:
      `lacre-link:<clave del primario>:<token>[@dirección…]`.
    - El nuevo la escanea (o la pega) y pide entrar
      (`Request::Join { token, label }`). El primario comprueba el token,
      lo añade al registro y contesta con el registro firmado
      (`Response::Granted`).
    - En el móvil la pantalla empieza por «Escanear»; en el escritorio,
      por el QR. Los dos caminos están siempre disponibles, también entre
      dos móviles.
  - **La invitación** dura 10 minutos y sirve una sola vez. Solo existe
    en memoria y mientras está abierto el diálogo: al cerrarlo deja de
    valer. El token es de 128 bits y se compara por su hash BLAKE3, para
    no revelar nada por tiempos. Quien ve el QR puede vincular un
    dispositivo, igual que quien tiene el código y el primario a mano. Por
    eso se muestra solo mientras se está vinculando, y el dispositivo
    vinculado aparece en la lista y se puede revocar.
  - **Direcciones:** el código del nuevo y la invitación llevan las
    direcciones alcanzables desde otras redes. Hoy es la onion, en cuanto
    está publicada, y se usa el mismo formato que el QR de contacto
    (D-021). Los códigos antiguos, sin `@`, siguen valiendo.
    - La app vuelve a pedir el código cada 10 s y dice si ya es
      «Accesible desde otras redes» o, de momento, solo desde la red local.
  - **Resiliencia:**
    - Antes de tocar el registro se intenta llegar al otro dispositivo
      durante hasta 150 s: por sus direcciones (se vuelve a marcar cada
      20 s), la red local o la DHT.
    - Si el aviso final falla, se quita del registro el dispositivo recién
      añadido.
    - Los fallos llegan a la app como `LinkResult` (`BadCode`,
      `Unreachable`, `Refused`) y se explican con un mensaje claro en lugar
      del error en crudo.
  - En la CLI se usan `invite` y `join <invitación> [etiqueta]`. `whoami`
    enseña el código con direcciones mientras no hay identidad.
- **Limitaciones:**
  - Entre redes distintas, sin onion lista no hay forma de llegar: hay que
    esperar a que el código diga «Accesible desde otras redes». Las
    direcciones de relay (`p2p-circuit`) no se incluyen.
  - El nuevo dispositivo elige su nombre («móvil»/«ordenador»); se puede
    ver en la lista del primario, pero aún no renombrar.
  - Un nodo anterior a D-037 no entiende `Join`: entre versiones
    distintas sigue valiendo el código del dispositivo nuevo.

## D-038 · Siempre conectado o comprobar cada cierto tiempo (Android)
- **Contexto:** aun con el modo de reposo (D-034), mantener el nodo y Tor
  vivos en segundo plano gasta mucha batería en Android. Se pidió que la
  persona elija entre seguir conectada o comprobar los mensajes cada cierto
  tiempo. Los detalles se decidieron sin preguntar.
- **Decisión:** en Perfil → Este dispositivo, un interruptor «Siempre
  conectado a Tor» (por defecto, como hasta ahora). Si se apaga, se elige
  cada cuánto comprobar: 15 min, 30 min, 1 h o 2 h.
  - El ajuste lo guarda Kotlin (`SharedPreferences`, clave `intervalo`; 0 es
    siempre conectado), porque lo necesita antes de que arranque Dart. Dart
    lo lee y lo cambia por el canal (`intervalo`, `fijarIntervalo`).
  - **Comprobación:** un `JobService` (`CheckService`) periódico de
    JobScheduler, con red y persistente tras reiniciar. Arranca el proceso
    si hace falta; `LacreApp` crea el motor y Dart arranca el nodo, que
    recoge los buzones en el primer tick y envía lo pendiente. A los 2 min
    acaba y, si no hay pantalla, se cierra el proceso. Sin dependencias
    nuevas (no WorkManager).
  - **Al salir de la app:** el nodo sigue 60 s con el servicio en primer
    plano, para terminar de enviar, y después se para el servicio y se
    cierra el proceso, como el botón «Cerrar» del aviso. El servicio se
    arranca al abrir la pantalla, porque desde segundo plano Android no deja
    arrancar servicios en primer plano. Durante una comprobación no lo hay:
    el proceso vive mientras dura el job.
  - Con intervalo no se usa el modo de reposo: el nodo solo corre en
    segundo plano durante la gracia o una comprobación, y conviene que sean
    rápidas.
- **Limitaciones:**
  - Los mensajes llegan con retraso: hasta el intervalo, y más con el
    móvil en Doze, que agrupa los jobs en sus ventanas de mantenimiento.
    15 min es el mínimo de JobScheduler.
  - Mientras está cerrado no se es buzón para otros ni se responde a la
    DHT.
  - Si una comprobación no basta (Tor tarda en arrancar, buzones lentos),
    lo que falte llega en la siguiente.

## D-039 · Vincular solo con un secreto de un solo uso; sin vuelta atrás del registro
- **Contexto:** revisión de seguridad interna. Hasta ahora el dispositivo
  nuevo aceptaba el primer `LinkGrant` válido que lo nombrara (D-011). Quien
  supiera su clave pública (que viaja en el código de vincular y en la red)
  podía adelantarse y meterlo en una identidad ajena. Además, un `LinkGrant`
  con un registro viejo de la misma raíz deshacía revocaciones.
- **Decisión:**
  - El código de vincular pasa a ser `<clave del dispositivo>:<secreto>@direcciones`.
    El secreto son 16 bytes aleatorios (base32). Lo genera el dispositivo
    nuevo y dura 10 min (`INVITE_TTL`), como las invitaciones de D-037.
  - El `LinkGrant` lleva el secreto (`proto::LinkGrant { record, secret }`).
    El dispositivo nuevo solo lo acepta si:
    1. lo envía un dispositivo que está en el registro que trae;
    2. el secreto coincide, comparando sus hashes BLAKE3 (tiempo constante).
       Se consume al usarlo: un solo uso.
  - Las invitaciones (`invite`/`join`) usan el mismo mecanismo
    (`current_token`/`take_token`).
  - `accept_grant` rechaza un registro de la misma raíz con `seq` menor que el
    que ya tenemos.
- **Motivo:** el código de vincular solo lo ve quien tiene delante la pantalla
  del dispositivo nuevo. Ligar la aceptación a un secreto de ese canal
  convierte el código en una autenticación real.
- **Limitaciones:**
  - El código sigue siendo secreto durante 10 min: quien lo fotografíe en
    ese tiempo puede usarlo antes. Es de un solo uso, así que el dueño lo
    notaría: el suyo fallaría.
  - El formato del código cambia: un código de una versión anterior no vale.
- **Pendiente:** confirmación visual en los dos dispositivos (un número corto
  que se compara), como Signal.

## D-040 · El remitente MLS tiene que estar en el registro de su raíz
- **Contexto:** OpenMLS comprueba las firmas, pero la credencial de cada
  hoja la pone quien la crea. Un miembro malicioso, o un dispositivo revocado
  aún en el árbol, podía añadir hojas con la credencial de otra persona.
  También podía hacerse pasar por el committer en un Welcome.
- **Decisión:**
  - **Núcleo (`mls.rs`):**
    - Solo se aceptan mensajes con `Sender::Member`.
    - La credencial del remitente se saca de su hoja (`member_at`) y tiene
      que corresponder a su clave de firma (`member_of`). Además, tiene que
      coincidir con la del mensaje procesado.
    - Antes de aplicar un commit se valida `member_of` en todas las hojas que
      trae: altas, actualizaciones y la de la ruta del commit.
  - **Nodo:**
    - Un mensaje de aplicación cuyo dispositivo sabemos que no pertenece a esa
      raíz (por nuestra identidad o por los contactos) es una falsificación.
      No cuenta como propio, no dispara salidas y no actualiza fotos.
    - Un Welcome solo se acepta automáticamente si el committer está
      verificado. Vale que su dispositivo esté en el registro conocido o, si
      no lo está, en el registro firmado de su raíz resuelto de la DHT
      (`device_verified`).
    - `verify_members` avisa cuando no puede leer los miembros, en vez de
      callar.
- **Motivo:** en Lacre la identidad es la raíz (D-002). Una hoja MLS solo es
  de alguien si el registro firmado de su raíz incluye ese dispositivo.
- **Limitaciones:**
  - Si el registro de una raíz no se puede resolver (DHT caída), un Welcome
    de ese committer queda como solicitud en vez de aceptarse solo.
  - Un Welcome antiguo puede reenviarse mientras siga valiendo el KeyPackage
    de último recurso (hasta 7 días). Volver a entrar en un grupo del que se
    salió necesita además que el committer lo firmara.

## D-041 · Fijar la raíz de cada contacto y no elegir ante la duda
- **Contexto:** el sufijo del handle tiene 40 bits (D-002). Con suficiente
  cómputo se puede encontrar otra raíz con el mismo handle. Hasta ahora, ante
  varios registros válidos para un handle se elegía uno (`best_record`).
- **Decisión:**
  - `resolve_pinned(handle, raíz)`:
    - Si ya conocemos el handle (un contacto o nosotros), se fija su raíz y se
      descartan los registros de otras.
    - Si un QR trae una raíz distinta de la fijada, es un error, no un cambio
      silencioso.
  - `select_record`:
    - Primero un filtro barato: nombre y raíz fijada.
    - Después verifica como mucho 16 candidatos (`MAX_CANDIDATES`).
    - Si quedan registros válidos de raíces distintas, es un error («handle
      ambiguo»): no se elige ninguno.
  - Crear un chat desde un QR usa la raíz del QR.
- **Motivo:** confiar en el primer uso (TOFU) y en el QR, como las huellas de
  seguridad de otras apps, en lugar de depender solo del coste de Argon2.
- **Limitaciones:**
  - El primer contacto por handle (sin QR) sigue confiando en lo que haya en
    la DHT. Un atacante que haya publicado una colisión antes puede provocar
    el error de ambigüedad, pero no suplantar en silencio.
  - No hay todavía una pantalla de «número de seguridad» para comparar
    raíces en persona.

## D-042 · Límites de recursos, firmas estrictas y cifrado en disco v2
- **Contexto:** revisión de seguridad interna, con un nodo malicioso como
  atacante (D-031). Encontró formas de agotar CPU, memoria y disco, y algunos
  huecos de validación. Mitiga en parte R-007 (disponibilidad) del registro de
  riesgos.
- **Decisión:**
  - **Firmas y claves:**
    - Registros y sobres se verifican con `verify_strict`, que rechaza firmas
      maleables.
    - Se rechazan claves débiles (de orden pequeño) como raíz, dispositivo o
      remitente.
    - El registro valida sus dispositivos: entre 1 y 16, sin duplicados,
      ninguno igual a la raíz, y nombres de ≤64 caracteres sin caracteres de
      control.
    - El nombre se valida antes de calcular Argon2.
  - **Argon2:** un solo cálculo a la vez (cerrojo global). La caché se vacía
    al llegar a 4096 entradas.
  - **DHT:**
    - Un `PutRecord` idéntico al guardado no se vuelve a validar.
    - Cada par tiene un presupuesto de 20 por 10 min.
    - Como mucho 2 validaciones a la vez.
  - **Peticiones:**
    - KeyPackages: 8 por par y 100 en total por hora.
    - Depósitos en buzón: el mismo presupuesto que las entregas.
    - `SyncHint`: solo de dispositivos del grupo o de una raíz miembro, y uno
      cada 20 s por grupo.
    - Como mucho 64 peticiones entrantes a la vez; las demás se descartan.
    - Los errores devueltos al otro llevan solo el mensaje externo, acotado
      a 200 caracteres.
  - **Conexiones:** `connection_limits` de libp2p:
    - 64 entrantes pendientes.
    - 256 establecidas (2048 en un relay).
    - 4 por par.

    Las tablas `dialed` y `hops` se limitan a 1024 entradas: al llenarse se
    olvidan los pares sin conexión.
  - **Buzones:**
    - 16 MiB por destinatario (se descartan los sobres más antiguos) y
      256 MiB en total («buzón lleno»).
    - Se escriben a disco como mucho cada 10 s, y también antes de entregar.
  - **Contenido:**
    - Textos de hasta 16 Ki caracteres.
    - De una solicitud sin aceptar se guardan como mucho 50 mensajes, sin
      stickers.
    - Como mucho 50 solicitudes pendientes.
    - Cola de reintentos MLS con cupo por grupo (32) y para grupos
      desconocidos (32).
  - **Sobres:**
    - Se marcan como vistos después de abrirlos bien. Así, un sobre falso
      con el id de uno legítimo no lo bloquea.
    - **Fecha firmada (formato v3):** el sobre lleva `sent`, cubierto por la
      firma del remitente. Se rechaza si tiene más de 15 días (un buzón
      guarda 14) o más de 1 día en el futuro.
    - Los ids vistos se recuerdan hasta que su sobre caduca (máx. 20 000).
      Bajo inundación se olvida el más antiguo y sube un «suelo» de fecha.
      Con eso, ninguna repetición se acepta nunca, ni siquiera tras salir
      de la lista. Antes se recordaban los últimos 5000 sin fecha, y una
      repetición posterior a esos 5000 se aceptaba.
    - Es incompatible con v2 (cambian los dominios de firma y KDF). Como no
      hay versión publicada, no se mantiene compatibilidad; los sobres v2
      que queden en buzones se descartan al abrirlos.
  - **Punto de encuentro:** solo se marcan direcciones públicas que vengan de
    trackers o de Mainline. Nada de red privada, loopback, enlace local,
    CGNAT ni multicast, para que un tracker no nos use contra la red local.
  - **Nombres de archivo:** además de lo de D-026, se evitan los nombres
    reservados de Windows (`CON`, `NUL`, `COM1`…) y los puntos y espacios
    finales.
  - **Cifrado en disco v2 (sustituye el formato de D-033):**
    - Cabecera `P2MVLT2`. El AAD lleva la ruta relativa al directorio de
      datos, así que no se puede cambiar un archivo cifrado por otro.
    - Con clave, un archivo en claro o v1 es un error. Solo la migración al
      arrancar (`migrate`) los acepta y los reescribe en v2.
  - **Clave de datos en el escritorio:**
    - Linux: Secret Service.
    - macOS: Llavero.
    - Windows: DPAPI, en `clave.dpapi` con entropía propia.
    - Si no se puede obtener, la app **no arranca**: nunca se guarda en
      claro. Alternativa: `LACRE_DATA_KEY`. El nodo de línea de órdenes
      (relays) sigue pudiendo ir sin clave.
  - **Memoria:** las copias intermedias de las claves privadas se borran
    (`Zeroizing`). Las claves de `ed25519-dalek` ya se borran al soltarse.
  - **Entrar con un Welcome es atómico:** OpenMLS borra el KeyPackage al
    preparar el Welcome, antes de comprobarlo entero. Un Welcome alterado
    dejaba sin KeyPackage al válido. Ahora, si `join` falla por cualquier
    motivo, el estado MLS vuelve a como estaba. Lo encontraron las pruebas
    de robustez.
  - **Pruebas de robustez** (`crates/lacre-core/tests/robustez.rs`): miles de
    entradas aleatorias y mutaciones de entradas válidas. Cubren registros,
    sobres, Welcomes, mensajes MLS y KeyPackages, y comprueban que nada entra
    en pánico ni se acepta alterado. La semilla es fija, así que un fallo se
    puede repetir.
  - **Fuzzing guiado por cobertura** (`fuzz/`, objetivos de `cargo fuzz`):
    `registro`, `sobre`, `mls` (Welcome, mensaje y KeyPackage), `peticion`
    (todo el CBOR del protocolo directo) y `destino` (handles y QR).
    `scripts/fuzz.sh` los compila con la instrumentación de libFuzzer en
    Rust estable y los ejecuta; con nightly, `cargo +nightly fuzz run`
    añade AddressSanitizer. Una primera pasada de 4 min por objetivo (unos
    150 millones de ejecuciones en total) no encontró fallos.
  - **CI:** el job `seguridad` ejecuta las pruebas, clippy (`correctness` y
    `suspicious` como errores), `cargo audit`, que falla con dependencias
    con avisos de RustSec, y 60 s de fuzzing por objetivo.
- **Motivo:** cada límite corta un ataque concreto de la revisión, con
  márgenes muy por encima del uso normal.
- **Limitaciones:**
  - **Disco** *(en parte resuelto por D-046)*: se puede volver a una
    versión anterior de un archivo cifrado (*rollback*): el AAD liga la
    ruta, no la versión.
  - **Sobres:** la fecha la pone el reloj del remitente. Uno con el reloj
    atrasado más de 15 días no puede enviar nada; uno adelantado más de un
    día, tampoco. Bajo inundación (más de 20 000 sobres vigentes) se puede
    perder un sobre legítimo muy retrasado.
  - **Argon2:** puede ejecutarse en hilos del runtime y bajo el cerrojo de
    estado. Un handle nuevo cuesta ~1 s.
  - **`SyncHint`:** la limitación puede descartar uno legítimo; la sincronía
    periódica lo cubre.
  - **Límites por par:** un atacante con muchas identidades de libp2p
    (Sybil) multiplica sus presupuestos. Los totales globales lo acotan.
  - **Claves en memoria:** las de larga vida (dispositivo, clave de datos,
    estado de OpenMLS) siguen en memoria mientras corre el proceso.
  - **Tor** *(sustituido por D-044: la clave onion ya no está en disco)*:
    arti no permite cifrar su almacén de claves, así que el estado
    de Tor (la clave del servicio onion) sigue en claro, con permisos
    `0700`. El impacto es acotado. Con esa clave se puede publicar el mismo
    `.onion` y cortar o desviar las conexiones que lleguen por él. No se
    puede suplantar al dispositivo: libp2p autentica con Noise la clave del
    dispositivo, que sí va cifrada. Tampoco se pueden leer mensajes.
  - **Vuelta atrás del disco:** solo se evitaría del todo con un contador
    monotónico del hardware (TPM, Secure Enclave). Quien puede escribir en el
    disco con el dispositivo apagado puede devolver todo el directorio a una
    copia anterior coherente, aunque no pueda leerla.
- **Pendiente:**
  - Auditoría externa del protocolo y del código.
  - Fuzzing largo y continuo (OSS-Fuzz o similar); el de CI es breve. El
    objetivo `sobre` no pasa del descifrado (es lo esperado: sin la clave
    no se puede fabricar un sobre válido), así que el CBOR interior se
    prueba solo con las pruebas de robustez.
  - Caducidad de los registros de la DHT. No basta con una fecha en el
    registro: quien guarda uno antiguo puede seguir sirviéndolo. Lo que
    protege es fijar la raíz (D-041) y la secuencia creciente del registro,
    que hace preferir siempre el más nuevo.
  - ~~Cifrar el estado de Tor cuando arti lo permita, o derivar la clave
    onion de la clave de dispositivo.~~ Hecho en D-044.

## D-043 · Número de seguridad
- **Contexto:** el sufijo del handle tiene 40 bits (D-003). Suplantar a una
  persona concreta es caro, pero no imposible para un atacante con muchos
  recursos. Encontrar dos personas cualesquiera con el mismo sufijo es
  barato (efecto cumpleaños). D-041 fija la raíz de cada contacto, pero el
  primer contacto por handle confía en la DHT.
- **Decisión:**
  - Número de seguridad de dos identidades: BLAKE3 en modo derivación con
    el contexto `p2message/safety-number/v1` sobre las dos `root_pk`
    ordenadas. De la salida extensible se toman 60 bytes en 12 trozos de 5;
    cada trozo, como entero big-endian módulo 100 000, da un grupo de 5
    cifras. Son 60 cifras, unos 199 bits, iguales en los dos lados.
  - Lacre lo muestra al tocar a una persona en «Miembros» (3 filas de 4
    grupos) y con `safety <handle>` en el nodo de línea de órdenes.
  - Si coincide al compararlo en persona o por otro canal, la raíz que
    tenemos fijada es la suya: nadie suplanta el handle.
  - El QR (D-021) sigue siendo la forma cómoda de fijar la raíz sin
    comparar números.
- **Motivo:** usa la clave completa, no el sufijo. Es el método de Signal,
  conocido por los usuarios.
- **Limitaciones:**
  - No se guarda si se ha verificado ni se avisa de forma especial si el
    número cambia. No puede cambiar sin cambiar la raíz, y un cambio de raíz
    ya es un error con D-041.
  - Depende de que la persona lo compare de verdad.
  - Sin QR del número: se compara leyéndolo.
- **Pendiente:** marcar contactos verificados y mostrarlo en el chat.

## D-044 · Clave onion derivada y solo en memoria
- **Contexto:** la clave del servicio onion la generaba Arti y la guardaba
  en claro en `<datos>/tor/state/keystore` (D-042, limitaciones). Con ella
  se puede publicar el mismo `.onion` y cortar o desviar las conexiones que
  lleguen por él.
- **Decisión:**
  - Arti usa el almacén de claves **efímero** (en memoria; opción
    `ephemeral-keystore` de arti-client).
  - La clave onion se deriva de la del dispositivo:
    `BLAKE3-KDF("p2message/onion-id/v1", device_sk)` como semilla Ed25519.
    Se entrega a Arti al arrancar (`launch_onion_service_with_hsid`). La
    dirección onion es la misma en cada arranque, sin guardar nada.
  - Al arrancar se borran `tor/state/keystore` (la clave en claro de
    versiones anteriores) y `tor/state/hss`, que apunta a claves de los
    puntos de introducción que ya no sobreviven a un reinicio.
  - En `tor/` solo quedan datos públicos: el directorio de la red, los
    guardas y registros de repetición.
- **Motivo:** la clave del dispositivo ya va cifrada (D-033, D-042). Derivar
  de ella con un dominio propio no reutiliza la clave para dos usos (D-008).
  Además, quien conoce la clave pública del dispositivo no puede calcular
  la dirección onion.
- **Limitaciones:**
  - Las opciones `experimental-api` y `ephemeral-keystore` de Arti son
    experimentales: una versión nueva puede cambiarlas. Cada actualización
    de Arti debe comprobarlas.
  - Quien robe la clave del dispositivo obtiene también la onion. Ya podía
    suplantar al dispositivo, así que no empeora nada.
  - Con la primera versión que trae esto, la dirección onion de cada nodo
    cambia una vez. Los demás la aprenden por identify y la DHT.
  - Los puntos de introducción se renuevan en cada arranque. En una prueba
    aquí, el descriptor se publicó igual que con el almacén en disco.
  - El sistema puede llevar la memoria al disco (swap, hibernación,
    volcados): Arti lo advierte y Lacre no lo evita.
- **Pendiente:** comprobar en Android que el servicio se publica igual tras
  el cambio.

## D-045 · Avisos de seguridad de dependencias y Arti 0.48
- **Contexto:** cruzando `Cargo.lock` con la base de avisos de RustSec
  (2026-11-26) salieron fallos conocidos en dependencias: rustls
  (degradación de intercambio de claves, secretos en `Debug`), rustls-webpki
  (intermedios desconfiados), tokio, url, zstd/zstd-safe/zstd-sys, rustix y
  ctutils.
- **Decisión:**
  - Actualizar todas las que tienen arreglo compatible (`cargo update`):
    rustls 0.23.46, rustls-webpki 0.103.17, tokio 1.54.1, url 2.5.9,
    zstd 0.14.5, zstd-safe 8.4.0, zstd-sys 2.2.0, rustix 1.1.6,
    ctutils 0.4.8.
  - Subir Arti de 0.47 a 0.48. Con 0.48 el servicio onion se publica en
    unos 10 s: con 0.47 seguía «arrancando» a los 240 s. La dirección onion
    derivada (D-044) se mantiene tras reiniciar.
    Probado aquí: un segundo nodo sin escucha directa (`--no-listen`) se
    conectó al primero por su dirección `/onion3/…`.
  - El CI ya pasa `cargo audit`: un aviso nuevo para la compilación.
- **Riesgos aceptados** (sin arreglo publicado que podamos usar):
  - **SQLite** (libsqlite3-sys 0.38, CVE-2026-119375): lo trae Arti para la
    caché del directorio de Tor, y Arti 0.48 sigue con rusqlite 0.40. Hace
    falta que alguien escriba una base y su `-wal` manipulados en
    `<datos>/tor/`. Quien puede hacer eso ya puede leer y cambiar los datos
    del dispositivo.
  - **rsa** (ataque Marvin, RUSTSEC-2023-0071): Arti solo verifica firmas
    RSA de la red Tor y no descifra con RSA. El ataque no se aplica.
  - **crypto-bigint 0.5** (tiempo no constante): lo usan p256/p384/k256 a
    través de openmls. Lacre usa la suite MLS con X25519 y Ed25519
    (`MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`): ese código no
    maneja secretos nuestros.
  - Crates sin mantenimiento (bincode, chrono, humantime, paste, zvariant…):
    no son fallos. Se revisan en cada actualización.
- **Pendiente:** pasar a rusqlite ≥ 0.41 cuando Arti lo haga; seguir los
  avisos en cada versión.

## D-046 · Número de guardado contra la vuelta atrás de archivos
- **Contexto:** el estado va cifrado y ligado a su ruta (D-033, D-042), pero
  no a su versión. Quien pueda escribir en el disco con la app cerrada puede
  devolver un archivo a una copia anterior, o borrarlo, sin la clave: por
  ejemplo, deshacer un bloqueo o volver a un estado de grupo viejo.
- **Decisión:**
  - Los archivos de estado importantes (`device.key`, `root.key`,
    `identity.cbor`, `mls.json`, `chats.cbor`, `seen.cbor`, `mailbox.cbor`,
    `history.cbor`, `contacts.cbor`, `lastresort.kp`, `blocked.cbor`,
    `stickers.cbor`) pasan al formato v3: `P2MVLT3\0 ‖ n (u64 BE) ‖ nonce ‖
    cifrado`, con `n` (número de guardado) dentro del dato asociado:
    `"p2message/vault/v3\0" ‖ n ‖ ruta`.
  - `vault.manifest` (cifrado, v2) guarda el último número usado y el de
    cada archivo. Cada guardado usa el siguiente número: primero se escribe
    el archivo y luego el registro.
  - Al leer, el número del archivo tiene que ser el del registro. Solo se
    admite además el siguiente al último (un corte entre las dos
    escrituras), y entonces se completa el registro. Un archivo que el
    registro conoce y falta, o que vuelve a v2, también se rechaza.
  - Al arrancar se leen todos los protegidos: el aviso sale al principio y
    el nodo no arranca.
  - Los archivos v2 de antes se reescriben en v3 al arrancar.
  - `--accept-current-state` (CLI) rehace el registro con lo que hay, para
    quien restaura a sabiendas su propia copia de seguridad.
- **Motivo:** cierra la vuelta atrás selectiva (un solo archivo, como la
  lista de bloqueados, con el resto al día), que es la útil para un
  atacante. Cuesta una escritura pequeña más por guardado.
- **Limitaciones:**
  - Devolver **todo** el directorio, registro incluido, a una copia
    anterior coherente no se detecta. Haría falta un contador monotónico del
    hardware (TPM, Secure Enclave). El llavero del sistema no sirve: también
    es un archivo del mismo disco.
  - Un único guardado desde la copia no se distingue de un corte entre el
    archivo y el registro.
  - Fotos, stickers, archivos y `peers.txt` no están protegidos: volver
    atrás en ellos no da nada a un atacante y se borran a menudo.
  - Hasta el primer arranque con esta versión, los archivos v2 se pueden
    cambiar por otros v2 anteriores.
- **Pendiente:** ~~en la app, un botón para aceptar el estado actual tras
  el aviso~~ → hecho: el aviso es un error tipado (`vault::Rollback`,
  `node::StateRollback`) y la pantalla de error de arranque lo explica y
  ofrece «Aceptar el estado actual», con confirmación.

## D-047 · Código de confirmación al vincular
- **Contexto:** D-011 pedía confirmación visual en los dos dispositivos al
  vincular. D-039 añadió el secreto de un solo uso, pero quien veía el
  código (o cambiaba el QR) podía vincular su dispositivo, o hacer que el
  nuevo entrase en otra identidad, sin que nada lo mostrase en el momento.
- **Decisión:**
  - Al terminar, los dos dispositivos muestran 6 cifras: `link-confirm =
    BLAKE3(derive_key "p2message/link-confirm/v1", raíz ‖ dispositivo ‖
    secreto)`, los 4 primeros bytes (u32 BE) mod 1 000 000, como `123 456`.
  - `raíz`: la clave raíz del registro aceptado. `dispositivo`: la clave
    del dispositivo nuevo. `secreto`: el del código de vincular (flujo A) o
    el token de la invitación (flujo B).
  - El primario lo calcula con el registro que firma. El nuevo lo calcula
    con el registro que recibe y su propia clave. La persona los compara.
  - App: diálogo en quien inicia la vinculación y aviso que no se va solo en
    el otro. CLI: `(confirmación: 123 456)`.
- **Motivo:** si alguien cambia el QR o se adelanta con el código, la raíz o
  el dispositivo no son los esperados y los códigos no coinciden. Así se ve
  en el momento, sin repasar la lista de dispositivos. No cambia el
  protocolo en la red: cada parte lo calcula con lo que ya tiene.
- **Limitaciones:**
  - Son 6 cifras. Un atacante que elija su clave de dispositivo y conozca
    el secreto podría probar claves hasta igualar el código que verá el
    dueño (unas 10⁶ claves Ed25519, rápido), pero tendría que saber de
    antemano qué código muestra el otro lado, que depende de la raíz real.
    Sirve para detectar errores y sustituciones, no como prueba
    criptográfica fuerte. El número de seguridad (D-043) sí lo es.
  - Solo sirve si la persona lo compara de verdad. Si no coincide, hay que
    desvincular el dispositivo a mano.

## D-048 · Frase de recuperación y traspaso del primario
- **Contexto:** D-004 preveía 24 palabras para la clave raíz. Sin ellas,
  perder el primario dejaba la identidad sin poder vincular ni desvincular
  dispositivos, y no había forma de pasar el papel de primario a otro.
- **Decisión:**
  - Frase: la clave raíz (32 bytes de semilla Ed25519) en BIP39, 24
    palabras de la lista inglesa con 8 bits de suma de comprobación (crate
    `bip39`). Se admiten mayúsculas y espacios de más.
  - Recuperar (`recover`): se deriva la raíz y se busca en la DHT el
    registro de su sufijo con esa `root_pk` y mayor `seq`. Este dispositivo
    entra en él y se firma con `seq = max(seq + 1, ahora_unix)`, por encima
    de cualquier copia que circule. Se guarda `root.key` y se publica.
    Después los committers añaden el dispositivo a los grupos en su próxima
    sincronización, como con un dispositivo vinculado.
  - `--only-this` / casilla en la app: el registro se queda solo con este
    dispositivo, para cuando se pierde o roban el anterior.
  - Vale en un dispositivo nuevo o en uno ya vinculado a esa identidad.
    Traspasar el primario = recuperar en el nuevo y `forget-root` en el
    viejo. `forget-root` borra `root.key` y la saca del registro de
    guardados (D-046, `vault::remove`) para que no se tome por un borrado
    ajeno.
  - App: frase en el perfil (palabras numeradas, con aviso), "Recuperar" en
    la primera pantalla, "Pasar a ser el primario" y "Dejar de ser el
    primario" en el perfil. CLI: `phrase`, `recover`, `forget-root`.
- **Motivo:** formato estándar, con suma de comprobación y una lista de
  palabras pensada para anotarse a mano. `seq` basado en la hora evita que
  un registro viejo, guardado en algún nodo, gane al recuperado.
- **Limitaciones:**
  - Quien tenga la frase es el dueño de la identidad. Si se filtra, no hay
    rotación posible (el handle sale de la raíz): el atacante puede firmar
    registros con `seq` mayores. La única salida es una identidad nueva.
  - Recuperar necesita encontrar el registro en la red: sin conexión con
    ningún nodo que lo tenga, no se sabe el nombre ni los dispositivos.
  - Los grupos cuyo committer era un dispositivo perdido siguen congelados
    hasta que se pueda traspasar el committer (hoja de ruta, punto 4). El
    historial anterior no llega al dispositivo recuperado (punto 6).
  - En Android la frase se muestra con FLAG_SECURE (sin capturas ni vista
    previa en recientes). En el escritorio no hay forma general de
    impedirlo.
  - Solo lista inglesa.

## Preguntas abiertas
- **Contribuciones:** ¿CLA o DCO? Afecta a la vía de la App Store (D-014).
