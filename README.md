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

DopePad is a tiny GUI notepad for quick notes, todo dumps, random ideas, and all that "I need this saved right now" stuff.

Hit a hotkey, type, done.

It is fast, keyboard-first, and intentionally simple.

## Why it exists

- open in a flash (especially with the background daemon)
- write with markup that stays visible, styled in place
- daily note + one-off notes without building a second brain
- also opens normal `.txt` / `.md` when you need a plain file
- looks at home on a dark Murasaki setup

## What you get

- native GTK4 / libadwaita window
- paper-first UI (chrome stays out of the way)
- daily notes and loose notes under `~/.local/share/dopepad/notes/`
- `.dpad` files with light frontmatter (hidden while you write)
- **DopeSyntax** inline styles — markers stay in the file
- autosave
- note picker (`Ctrl+P`) and find in note (`Ctrl+F`)
- single-instance app + optional `--daemon` so hotkeys feel instant

## Install

Deps (Debian / similar):

```bash
sudo apt install build-essential pkg-config \
  libgtk-4-dev libadwaita-1-dev
```

Then:

```bash
./scripts/install.sh
```

What it does:

- builds a release `dopepad`
- links it into `~/.local/bin`
- installs a desktop entry
- sets up PATH in your shell profile if needed

Then just run:

```bash
dopepad
```

## Uninstall

```bash
./scripts/uninstall.sh
```

Optional full cleanup (including your notes):

```bash
./scripts/uninstall.sh --purge-data
```

## Run

```bash
dopepad                 # today's daily note
dopepad --daily         # same
dopepad --new           # brand new loose note
dopepad something.txt   # plain file, saved as-is
dopepad note.dpad       # managed note
dopepad --daemon        # warm in background (no window)
```

## Notes storage

Managed notes live here:

```text
~/.local/share/dopepad/notes/
  daily_YYYY-MM-DD.dpad
  note_YYYY-MM-DD_HHMMSS.dpad
```

On disk a managed note looks roughly like this (frontmatter is hidden in the UI):

```text
---
kind: daily
created_at: 2026-07-09T00:00:00-03:00
updated_at: 2026-07-09T00:00:00-03:00
---

# Daily · 2026-07-09
```

Window titles:

```text
DopePad · Daily · 2026-07-09
DopePad · Note · 2026-07-09 02:14
DopePad · readme.txt
```

### Plain files

Open any path. If it is not a DopePad note, the buffer is the file.

```bash
dopepad ~/Notes/ideas.txt
dopepad README.md
```

## Niri / hotkeys

Suggested binds (config lives on your machine; DopePad does not rewrite it for you):

```kdl
spawn-at-startup "dopepad" "--daemon"

Mod+Alt+N   { spawn "dopepad" "--daily"; }
Mod+Shift+N { spawn "dopepad" "--new"; }
```

The daemon pays the GTK tax once at login. After that, hotkeys should feel snappy.

## DopeSyntax

Markers stay in the file. The editor just paints on top.

| you type | you see |
|----------|---------|
| `# Title` | big heading |
| `## Section` | medium heading |
| `### Sub` | smaller heading |
| `- item` | bullet |
| `- [ ] task` | open task |
| `- [x] task` | done (muted + strike) |
| `> quote` | thought / quote |
| `! alert` | callout |
| `==highlight==` | highlight |
| `**bold**` | bold |
| `` `code` `` | code |
| `@tag` | tag |

## Keys

| key | what |
|-----|------|
| `Ctrl+S` | save now |
| `Ctrl+N` | new note |
| `Ctrl+D` | daily |
| `Ctrl+P` | search notes in the vault |
| `Ctrl+F` | find in this note |
| `Esc` | close overlay / find |
| `Ctrl+Q` | quit |
| `Alt` / hover top | peek header |

Autosave kicks in shortly after you pause typing.

## Speed check

```bash
cargo build --release
./scripts/bench-launch.sh --daily
```

Cold process without a daemon is mostly GTK waking up. Keep `--daemon` on session start if you care about hotkey feel.

## Dev

```bash
cargo fmt
cargo test
cargo build --release
```

## Stack
Rust, GTK4, libadwaita, plain `TextView` / `TextBuffer`.