# Hoja de ruta

Lo que queda por hacer, sacado de los «Pendiente» y «Limitaciones» de
[DECISIONES.md](DECISIONES.md). Cada punto indica la decisión de la que
viene. Cuando se haga algo, se marca aquí y se apunta la decisión nueva en
DECISIONES.md.

## Imprescindible para el uso diario

1. **Android en segundo plano** (D-023): ahora el nodo solo vive con la app
   abierta. Servicio en primer plano con notificación fija y avisos de
   mensajes nuevos. → **Hecho (D-032).** Queda: escritorio en segundo
   plano (punto 16).
2. **Cifrado en disco** (D-012, D-022): historial, estado MLS, claves y
   demás estado en claro en el disco. Cifrarlo con una clave guardada en el
   almacén del sistema (Keystore en Android, Secret Service en Linux).
   → **Hecho (D-033, D-042)**: llavero también en macOS (Llavero) y
   Windows (DPAPI); sin clave la app no arranca. Falta Tor (punto 8) e iOS.
3. **Frase de recuperación** (D-004): 24 palabras para la clave raíz. Sin
   ella, si se pierde el dispositivo primario no se pueden vincular
   dispositivos nuevos. También: transferir el rol de primario.
   → **Hecho (D-048).** Recuperar, quitar los demás dispositivos y pasar el
   papel de primario (recuperar en el nuevo y borrar la clave del viejo).
4. **Transferir el rol de committer** (D-007, D-031): si el committer de un
   grupo desaparece, no se puede añadir, quitar ni salir. También:
   administradores múltiples y resolución de bifurcaciones.
   → **Traspaso hecho (D-049):** a mano, al salir del grupo y automático
   cuando se revoca el dispositivo gestor. Quedan los administradores
   múltiples y la resolución de bifurcaciones.

## Seguridad

5. **Vinculación segura** (D-011): QR bidireccional con secreto de un solo
   uso y confirmación visual en los dos dispositivos. → **Secreto de un solo
   uso hecho (D-039). Confirmación visual hecha (D-047).**
6. **Historial al vincular** (D-011): un dispositivo nuevo solo ve los
   mensajes posteriores a su entrada. → **Hecho (D-050):** se lo pasa
   otro dispositivo propio. Queda una copia de seguridad cifrada.
7. **Límite de escrituras en la DHT** (D-005): validar un registro cuesta un
   Argon2; limitar la tasa por peer. → **Hecho (D-042).** Queda probar en
   redes grandes y frente a Sybil.
8. **Fotos, stickers y archivos cifrados en disco** (D-033): → **Hecho**
   (las copias para abrir con otra app van a `abiertos/` y se borran). La
   clave onion ya no se guarda en disco (D-044). Vuelta atrás de un archivo
   suelto detectada (D-046). Queda la del directorio entero (necesita un
   contador del hardware). Botón de la app para aceptar el estado hecho.
8b. **Auditoría externa** (D-042): del protocolo y del código, antes de
   recomendar Lacre para uso real. Hechos: `cargo audit` en CI, pruebas de
   robustez y fuzzing guiado por cobertura de los decodificadores (también
   en CI), fecha firmada en los sobres (D-042) y número de seguridad
   (D-043). Quedan la auditoría en sí, fuzzing continuo y marcar contactos
   verificados.

## Red

9. **AutoNAT** (D-018, D-019): un `--relay-server` tras NAT se anuncia con
   una dirección inalcanzable; anunciarse solo tras confirmar que se es
   alcanzable. → **Hecho (D-051):** AutoNAT v1, UPnP o `--external`.
10. **IPv6** (D-019): en móvil suele haber IPv6 sin NAT, lo que facilita el
    hole punching.
11. **Tor en el móvil** (D-020, D-021, D-023): ~~coste de batería~~
    **(modo de reposo, D-034)**. Queda: mostrar «conectando a Tor…» mientras
    se publica el servicio onion; comprobar desde fuera que sigue publicado.
12. ~~**mDNS en el móvil** (D-023): no lanzarlo en interfaces de datos
    móviles.~~ **Hecho (D-034):** solo con wifi y en primer plano.
13. **Archivos con el remitente desconectado** (D-026): solo se descarga de
    dispositivos en línea. Buzones que guarden trozos cifrados un tiempo.

## Funciones

14. **Stickers** (D-025): paquetes compartibles, un paquete por defecto en
    la app y convertir GIF a WebP animado.
15. **iOS** (D-009): app y notificaciones con APNs (notificaciones vacías,
    inevitable en iOS).
16. **Escritorio en segundo plano**: en Linux el nodo se cierra con la
    ventana (icono en la bandeja o servicio de usuario).

## Por decidir

17. **Contribuciones:** ¿CLA o DCO? Afecta a la vía de la App Store (D-014).
