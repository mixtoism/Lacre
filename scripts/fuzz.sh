#!/usr/bin/env bash
# Fuzzing guiado por cobertura de los decodificadores (D-042), con Rust
# estable: compila los objetivos de fuzz/ con la instrumentación de libFuzzer
# (sancov) y ejecuta cada uno SEGUNDOS. Sin AddressSanitizer, que necesita
# nightly (`cargo +nightly fuzz run <objetivo>`); el código de Lacre no usa
# `unsafe`, así que lo que se busca aquí son pánicos, desbordamientos,
# bucles y consumo de memoria.
#
# Uso: scripts/fuzz.sh [SEGUNDOS] [objetivo…]
# Los fallos quedan en fuzz/artifacts/<objetivo>/; el corpus, en fuzz/corpus/.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
SECS=${1:-60}; shift || true
TARGETS=("${@:-registro sobre mls peticion destino}")
read -ra TARGETS <<< "${TARGETS[*]}"
TRIPLE=$(rustc -vV | sed -n 's/^host: //p')
export RUSTFLAGS="-Cpasses=sancov-module -Cllvm-args=-sanitizer-coverage-level=4 \
 -Cllvm-args=-sanitizer-coverage-inline-8bit-counters -Cllvm-args=-sanitizer-coverage-pc-table \
 -Cllvm-args=-sanitizer-coverage-trace-compares --cfg fuzzing -Cdebug-assertions -Coverflow-checks"
# --target explícito: así RUSTFLAGS no se aplica a los build scripts.
cargo build --release --bins --target "$TRIPLE" --manifest-path "$ROOT/fuzz/Cargo.toml"
BIN=$ROOT/fuzz/target/$TRIPLE/release
fail=0
for t in "${TARGETS[@]}"; do
  mkdir -p "$ROOT/fuzz/corpus/$t" "$ROOT/fuzz/artifacts/$t"
  echo "== $t ($SECS s)"
  if ! "$BIN/$t" "$ROOT/fuzz/corpus/$t" -max_total_time="$SECS" -rss_limit_mb=4096 \
      -artifact_prefix="$ROOT/fuzz/artifacts/$t/" -print_final_stats=1 2>&1 | tail -4; then
    fail=1
  fi
  if [ -n "$(ls -A "$ROOT/fuzz/artifacts/$t")" ]; then echo "FALLO en $t"; fail=1; fi
done
exit $fail
