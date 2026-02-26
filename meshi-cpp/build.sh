#!/usr/bin/env bash
set -euo pipefail

SOURCE_DIR="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)}"

cmake -G "Ninja" -DCMAKE_EXPORT_COMPILE_COMMANDS=TRUE "${SOURCE_DIR}"
cmake --build .
