#!/usr/bin/env bash
set -euo pipefail

APP_NAME="dopepad"
BIN_DIR="${HOME}/.local/bin"
LINK_PATH="${BIN_DIR}/${APP_NAME}"
CARGO_BIN="${HOME}/.cargo/bin/${APP_NAME}"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS_DIR="${DATA_HOME}/applications"
DESKTOP_DST="${APPS_DIR}/io.github.phpont.DopePad.desktop"
PATH_BLOCK_START="# >>> dopepad path >>>"
PATH_BLOCK_END="# <<< dopepad path <<<"
REMOVE_DATA=0

if [[ "${1:-}" == "--purge-data" ]]; then
  REMOVE_DATA=1
fi

remove_path_block() {
  local profile="$1"
  [[ -f "$profile" ]] || return 0

  if grep -Fq "$PATH_BLOCK_START" "$profile"; then
    local tmp
    tmp="$(mktemp)"
    awk -v start="$PATH_BLOCK_START" -v end="$PATH_BLOCK_END" '
      $0 == start {skip=1; next}
      $0 == end {skip=0; next}
      !skip {print}
    ' "$profile" >"$tmp"
    mv "$tmp" "$profile"
  fi
}

main() {
  rm -f "$LINK_PATH"
  rm -f "$DESKTOP_DST"

  if command -v cargo >/dev/null 2>&1; then
    cargo uninstall "$APP_NAME" >/dev/null 2>&1 || true
  fi
  rm -f "$CARGO_BIN"

  remove_path_block "${HOME}/.bashrc"
  remove_path_block "${HOME}/.profile"
  remove_path_block "${HOME}/.zshrc"
  remove_path_block "${HOME}/.zprofile"
  remove_path_block "${HOME}/.config/fish/config.fish"

  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APPS_DIR" >/dev/null 2>&1 || true
  fi

  if [[ "$REMOVE_DATA" -eq 1 ]]; then
    rm -rf "${DATA_HOME}/dopepad"
    echo "DopePad uninstalled and data removed."
  else
    echo "DopePad uninstalled."
    echo "User data kept at: ${DATA_HOME}/dopepad"
  fi

  echo "Restart your shell or open a new terminal session."
}

main "$@"
