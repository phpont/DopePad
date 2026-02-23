# Architecture

Quick map of how DopePad is split up.

## Layers

- `core/`: text buffer (Ropey), cursor/viewport, edits, search, character colors
- `io/`: load/save text files, EOL detection/preserve, sidecar read/write
- `input/`: raw key event -> command mapping
- `ui/`: Ratatui rendering (sidebar, editor, status bar, overlays)
- `app/`: event loop + orchestration between everything

Rule of thumb: `app` coordinates, `core` stays clean and terminal-agnostic.

## File tree and categories

Tree is generated from folders/files under:

- `~/.local/share/dopepad/notes/`

No hardcoded categories.

Tree node types:

- category
- file
- empty placeholder

## Main flows

- Open file: focus tree -> select -> `Enter`
- New note: `Ctrl+N` or `N` in tree
- New category: `C` in tree
- Delete note: `Del`/`D` in tree + confirmation modal
- Save As: choose file name + category in overlay

## Terminal safety

- raw mode + alternate screen
- drop guard for restore
- panic hook for restore on crash
