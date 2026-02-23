```text
▓█████▄  ▒█████   ██▓███  ▓█████  ██▓███   ▄▄▄      ▓█████▄
▒██▀ ██▌▒██▒  ██▒▓██░  ██▒▓█   ▀ ▓██░  ██▒▒████▄    ▒██▀ ██▌
░██   █▌▒██░  ██▒▓██░ ██▓▒▒███   ▓██░ ██▓▒▒██  ▀█▄  ░██   █▌
░▓█▄   ▌▒██   ██░▒██▄█▓▒ ▒▒▓█  ▄ ▒██▄█▓▒ ▒░██▄▄▄▄██ ░▓█▄   ▌
░▒████▓ ░ ████▓▒░▒██▒ ░  ░░▒████▒▒██▒ ░  ░ ▓█   ▓██▒░▒████▓
▒▒▓  ▒ ░ ▒░▒░▒░ ▒▓▒░ ░  ░░░ ▒░ ░▒▓▒░ ░  ░ ▒▒   ▓▒█░ ▒▒▓  ▒
░ ▒  ▒   ░ ▒ ▒░ ░▒ ░      ░ ░  ░░▒ ░       ▒   ▒▒ ░ ░ ▒  ▒
░ ░  ░ ░ ░ ░ ▒  ░░          ░   ░░         ░   ▒    ░ ░  ░
░        ░ ░              ░  ░               ░  ░   ░
░                                                   ░
```

# DopePad

DopePad is a terminal notepad that i developed for myself for quick notes, todo dumps, random ideas, and all that "I need this saved right now" stuff.

It is fast, keyboard-first, and intentionally simple.

## Why it exists

- I just wanted plain `.txt` files, no weird format lock-in.
- Categories and file browsing without leaving the terminal.
- Some color while writing, but without polluting the text file.

## What you get

- Rope-based editor core (`ropey`) for safe Unicode editing
- Sidebar tree for categories + notes
- Create category / create note / open / delete from the tree
- Search, goto line, help overlay
- Character-level colors with sidecar persistence

## Install

```bash
./scripts/install-path.sh
```

Or the direct way:

```bash
cargo install --path .
```

## Run

```bash
dopedpad
dopedpad notes.txt
dopedpad --readonly notes.txt
dopedpad --no-style notes.txt
```

## Notes storage

Everything lives under:

- `~/.local/share/dopedpad/notes/`

Fresh install is clean. No default categories.

## Tree keys (left panel)

- `Ctrl+O`: focus/unfocus tree
- `Up/Down`: navigate
- `Enter`: open selected note
- `N`: new note in selected category
- `C`: new category
- `Del` or `D`: delete selected note (with confirmation)
- `Esc`: back to editor

## Editor keys

- `Ctrl+N`: new note flow
- `Ctrl+S`: save
- `Ctrl+Shift+S`: save as
- `Ctrl+Q`: quit (asks if you have unsaved changes)
- `Ctrl+F`: search
- `Ctrl+G`: goto line
- `F1`: help
- `F2..F9`: set character color (`C1..C8`)
- `F10`: reset character color (`C0`)

## Dev

```bash
cargo build
cargo test
```
