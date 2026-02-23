# File Format

## Main note files

Your notes are normal `.txt` files.

- internal editor newline is `\n`
- save preserves detected original EOL (`LF` or `CRLF`)

## Sidecar styles

For `idea.txt`, style metadata goes to:

- `idea.txt.dopepad.json`

Example:

```json
{
  "char_colors": {
    "0": 3,
    "12": 5
  }
}
```

- value: color id (`1..8`)

If `--no-style` is enabled, sidecar is ignored (read/write).
