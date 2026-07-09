#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="dopepad"
BIN_DIR="${HOME}/.local/bin"
CARGO_BIN="${HOME}/.cargo/bin/${APP_NAME}"
LINK_PATH="${BIN_DIR}/${APP_NAME}"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS_DIR="${DATA_HOME}/applications"
DESKTOP_SRC="${ROOT}/data/io.github.phpont.DopePad.desktop"
DESKTOP_DST="${APPS_DIR}/io.github.phpont.DopePad.desktop"
NOTES_DIR="${DATA_HOME}/dopepad/notes"
PATH_BLOCK_START="# >>> dopepad path >>>"
PATH_BLOCK_END="# <<< dopepad path <<<"

ensure_path_block_posix() {
  local profile="$1"
  local block
  block="${PATH_BLOCK_START}\nexport PATH=\"\$HOME/.local/bin:\$PATH\"\n${PATH_BLOCK_END}"

  mkdir -p "$(dirname "$profile")"
  touch "$profile"

  if grep -Fq "$PATH_BLOCK_START" "$profile"; then
    return
  fi

  {
    echo ""
    printf '%b\n' "$block"
  } >>"$profile"
}

ensure_path_block_fish() {
  local profile="$1"
  local block
  block="${PATH_BLOCK_START}\nif not contains -- \$HOME/.local/bin \$PATH\n    set -gx PATH \$HOME/.local/bin \$PATH\nend\n${PATH_BLOCK_END}"

  mkdir -p "$(dirname "$profile")"
  touch "$profile"

  if grep -Fq "$PATH_BLOCK_START" "$profile"; then
    return
  fi

  {
    echo ""
    printf '%b\n' "$block"
  } >>"$profile"
}

pick_posix_profile() {
  local shell_name="$1"
  local candidates=()

  case "$shell_name" in
    bash)
      candidates=("${HOME}/.bashrc" "${HOME}/.profile")
      ;;
    zsh)
      candidates=("${HOME}/.zshrc" "${HOME}/.zprofile" "${HOME}/.profile")
      ;;
    *)
      candidates=("${HOME}/.profile")
      ;;
  esac

  for profile in "${candidates[@]}"; do
    if [[ -f "$profile" ]]; then
      echo "$profile"
      return
    fi
  done

  echo "${candidates[0]}"
}

install_binary() {
  echo "Building release binary…"
  (cd "$ROOT" && cargo install --path . --force)

  mkdir -p "$BIN_DIR"

  if [[ -x "$CARGO_BIN" ]]; then
    ln -sfn "$CARGO_BIN" "$LINK_PATH"
  else
    echo "Error: ${CARGO_BIN} not found after install." >&2
    exit 1
  fi
}

install_desktop() {
  mkdir -p "$APPS_DIR"
  if [[ -f "$DESKTOP_SRC" ]]; then
    sed "s|^Exec=dopepad|Exec=${LINK_PATH}|" "$DESKTOP_SRC" >"$DESKTOP_DST"
    echo "Desktop entry: ${DESKTOP_DST}"
  fi
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APPS_DIR" >/dev/null 2>&1 || true
  fi
}

configure_path() {
  local shell_name
  shell_name="$(basename "${SHELL:-sh}")"

  if [[ "$shell_name" == "fish" ]]; then
    local fish_profile="${HOME}/.config/fish/config.fish"
    ensure_path_block_fish "$fish_profile"
    echo "$fish_profile"
    return
  fi

  local profile
  profile="$(pick_posix_profile "$shell_name")"
  ensure_path_block_posix "$profile"
  echo "$profile"
}

main() {
  install_binary
  install_desktop
  mkdir -p "$NOTES_DIR"
  local profile
  profile="$(configure_path)"

  export PATH="${HOME}/.local/bin:${PATH}"

  echo ""
  echo "DopePad installed."
  echo "  Command:  dopepad"
  echo "  Notes:    ${NOTES_DIR}"
  echo "  PATH:     ${profile}"
  echo ""
  echo "Niri binds (add manually to your config):"
  echo "  Mod+Alt+N    dopepad --daily"
  echo "  Mod+Shift+N  dopepad --new"
  echo ""
  echo "If your shell was already open: source ${profile}"
}

main "$@"
