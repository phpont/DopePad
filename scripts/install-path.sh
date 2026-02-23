#!/usr/bin/env bash
set -euo pipefail

cargo install --path .

BIN_DIR="${HOME}/.local/bin"
mkdir -p "${BIN_DIR}"

if [[ -x "${HOME}/.cargo/bin/dopedpad" ]]; then
  ln -sf "${HOME}/.cargo/bin/dopedpad" "${BIN_DIR}/dopedpad"
fi

echo "DopePad instalado."
echo "Comando: dopedpad"
echo "Se necessario, adicione ao PATH: export PATH=\"$HOME/.local/bin:$PATH\""
