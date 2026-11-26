# Contribuir a Lacre

*English below.*

Lacre se publica bajo [GPL-3.0 o posterior](LICENSE). Las contribuciones se
aceptan con el **Developer Certificate of Origin** (DCO, versión 1.1,
<https://developercertificate.org/>) en lugar de un acuerdo de licencia
(D-058).

Cada commit debe llevar una línea que certifique que tienes derecho a
aportar ese código bajo la licencia del proyecto:

```
Signed-off-by: Nombre Apellido <correo@ejemplo.org>
```

`git commit -s` la añade sola. Para firmar commits ya hechos:
`git rebase --signoff <rama-base>`.

Antes de proponer un cambio:

- `cargo test`, `cargo clippy --all-targets` y `scripts/e2e.sh` pasan.
- En la app (`app/`), `flutter analyze` no da avisos.
- Las decisiones de diseño nuevas se anotan en
  [`docs/DECISIONES.md`](docs/DECISIONES.md) con su número (D-xxx).
- Los mensajes de registro (`tracing`) van en inglés; los errores que ve el
  usuario, la documentación y los comentarios, en español.

## Contributing (English)

Lacre is licensed under [GPL-3.0-or-later](LICENSE). Contributions are
accepted under the **Developer Certificate of Origin** 1.1
(<https://developercertificate.org/>), not a CLA (D-058). Every commit needs
a `Signed-off-by: Name <email>` line (`git commit -s`), certifying that you
have the right to submit it under the project's license.

Before sending a change, make sure `cargo test`, `cargo clippy --all-targets`,
`scripts/e2e.sh` and, for the app, `flutter analyze` pass. Log messages are
in English; user-facing errors, docs and code comments are in Spanish.
