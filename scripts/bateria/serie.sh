#!/bin/bash
# serie.sh salida minutos: una instantánea por minuto.
out=$1; n=$2
for i in $(seq 0 $n); do ./snap.sh >> "$out" 2>&1; [ $i -lt $n ] && sleep 60; done
