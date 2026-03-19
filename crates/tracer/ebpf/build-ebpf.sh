#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="${SCRIPT_DIR}/kernelwhisper.bpf.c"
OUT="${SCRIPT_DIR}/kernelwhisper.bpf.o"

clang -O2 -g -target bpf -c "${SRC}" -o "${OUT}"
echo "built ${OUT}"
