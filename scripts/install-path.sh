#!/usr/bin/env bash
set -euo pipefail

APP_NAME="dopepad"
BIN_DIR="${HOME}/.local/bin"
CARGO_BIN="${HOME}/.cargo/bin/${APP_NAME}"
LINK_PATH="${BIN_DIR}/${APP_NAME}"
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
  cargo install --path .

  mkdir -p "$BIN_DIR"

  if [[ -x "$CARGO_BIN" ]]; then
    ln -sf "$CARGO_BIN" "$LINK_PATH"
  else
    echo "Error: ${CARGO_BIN} not found after install." >&2
    exit 1
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
  local profile
  profile="$(configure_path)"

  # Make command available for the current shell session as well.
  export PATH="${HOME}/.local/bin:${PATH}"

  echo "DopePad installed."
  echo "Command: dopepad"
  echo "PATH configured in: ${profile}"
  echo "If your shell was already open, run: source ${profile}"
}

main "$@"
