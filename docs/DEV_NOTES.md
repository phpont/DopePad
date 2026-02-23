# Dev Notes

## Run locally

```bash
cargo run --
cargo run -- notes.txt
cargo run -- --readonly notes.txt
cargo run -- --no-style notes.txt
```

## Install command to PATH (auto)

```bash
./scripts/install-path.sh
```

The installer auto-detects shell profile (`bash`, `zsh`, `fish`) and writes PATH setup.

## Uninstall

```bash
./scripts/uninstall.sh
```

With user data purge:

```bash
./scripts/uninstall.sh --purge-data
```

## Tests

```bash
cargo test
```

Current test coverage includes:

- core edit/navigation behavior
- character-color map shift on insert/delete
- stress inserts
- EOL detect/preserve
- sidecar roundtrip

## UX pointers

- Sidebar is logo + hotkeys + tree
- Tree empty state prompts category creation
- All important actions are keyboard-first
- Confirmation modals use the same Yes/No interaction pattern
