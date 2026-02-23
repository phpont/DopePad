# Dev Notes

## Run locally

```bash
cargo run --
cargo run -- notes.txt
cargo run -- --readonly notes.txt
cargo run -- --no-style notes.txt
```

## Install command to PATH

```bash
./scripts/install-path.sh
```

## UX pointers

- Sidebar is logo + hotkeys + tree
- Tree empty state prompts category creation
- All important actions are keyboard-first
- Confirmation modals use the same Yes/No interaction pattern
