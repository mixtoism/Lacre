# Manifiesto de Lacre

*Borrador*

## Hablar en privado no es un privilegio

Durante siglos, una carta cerrada fue algo normal. Nadie tenía que justificar
por qué no quería que un tercero la leyera. Hoy casi toda nuestra conversación
pasa por servidores de unas pocas empresas. Esos servidores pueden ser
obligados a mirar, pueden equivocarse, pueden ser atacados y pueden cambiar
sus normas de un día para otro.

Cuando las conversaciones dependen de un punto central, la privacidad depende
de la buena voluntad de quien lo controla. Lacre nace para que no tenga
que ser así.

## Qué es

Lacre es un programa de mensajería entre iguales. No hay empresa, ni
servidor central, ni cuenta que alguien pueda cerrar.

- **Tu identidad es una clave que vive en tus dispositivos.** No es un número
  de teléfono ni un correo. Nadie te la asigna y nadie te la puede retirar.
- **Los mensajes se cifran de extremo a extremo con MLS (RFC 9420).** Solo
  pueden leerlos los dispositivos de quienes participan en la conversación.
- **Los nodos se encuentran y se hablan directamente**, a través de Tor por
  defecto. Cuando alguien está desconectado, otros nodos guardan sus mensajes
  sellados sin poder abrirlos.
- **Es software libre (GPL-3.0-or-later).** Cualquiera puede leer el código,
  comprobar que hace lo que dice, mejorarlo o continuarlo.

## Lo que creemos

1. **La privacidad de la correspondencia es un derecho**, no una sospecha. El
   artículo 7 de la Carta de los Derechos Fundamentales de la UE lo dice
   claramente.
2. **No existe una puerta trasera que solo usen los buenos.** Un sistema que
   puede leer todos los mensajes "por si acaso" puede leerlos también para
   cualquier otro fin, y lo hará quien llegue a controlarlo.
3. **La vigilancia masiva no sustituye a la investigación.** Escanear a todo
   el mundo trata a cada persona como sospechosa y llena de falsos positivos
   el trabajo de quien persigue delitos reales.
4. **La infraestructura debe ser de quien la usa.** Una red que no depende de
   nadie en concreto no puede ser apagada, vendida ni comprada.
5. **El código tiene que poder verificarse.** La confianza no se pide, se
   demuestra con código abierto, decisiones documentadas y construcciones
   reproducibles.

## Lo que no prometemos

Somos honestos sobre los límites:

- **Lacre no hace a nadie invisible.** Los nodos que guardan o retransmiten
  mensajes no ven el contenido, pero sí parte de los metadatos: quién escribe
  a quién, cuándo y cuánto. Seguimos trabajando para reducirlo.
- **Es una prueba de concepto.** No ha pasado una auditoría independiente. No
  lo uses todavía para proteger tu vida o tu libertad.
- **Un dispositivo comprometido lo compromete todo.** El cifrado no protege un
  teléfono con malware ni una pantalla vista por encima del hombro.

## Sobre el abuso

Cualquier herramienta de comunicación privada puede usarse para hacer daño, y
no lo ignoramos. Rechazamos el abuso, y de forma especial la explotación de
menores.

Pero la respuesta no puede ser leer las conversaciones de todos. Por eso
Lacre quiere poner el control en manos de cada persona (son funciones
previstas, aún no implementadas):

- bloquear a quien quiera;
- aceptar o rechazar mensajes de desconocidos;
- conservar y aportar como prueba lo que ha recibido, si decide denunciarlo.

Quien recibe un mensaje siempre puede mostrarlo. Lo único que no existe es un
tercero que lo lea sin permiso.

## Únete

Lacre no pertenece a nadie, ni a quien escribió sus primeras líneas.
Ejecuta un nodo, lee el código, rompe lo que esté roto y cuéntalo. Traduce,
documenta y comparte. Cuantos más nodos haya, más difícil será apagar la red.

Las razones de cada decisión técnica están en
[`docs/DECISIONES.md`](docs/DECISIONES.md).

---

*Compromiso de autoría (SHA-256):* `<hash de autoria.txt>`

*Firmado con la clave* `<huella de la clave del seudónimo>`
