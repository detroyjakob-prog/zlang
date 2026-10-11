# Z Programming Language — Website

A fast, dependency-free marketing/documentation site for the **Z** programming
language. The visual design mirrors the language's own CLI: a dark,
terminal-native aesthetic with cyan accents, rounded box-drawing panels, and a
monospace-forward type system.

## Files

| File | Purpose |
|------|---------|
| `index.html` | The single-page site (hero, features, syntax, tour, CLI, get-started). |
| `styles.css` | All styling — design tokens, layout, components, responsive rules. |
| `script.js` | Interactions — animated terminal, tabs, scroll reveal, copy button, mobile menu. |
| `assets/z-mark.svg` | The block-letter "Z" logo (echoes the CLI wordmark). |

No build step, no dependencies, no framework — just open it.

## Viewing it

Open `index.html` directly in a browser, or serve the folder:

```sh
# Python
python3 -m http.server 8000

# or Node
npx serve .
```

Then visit http://localhost:8000

## Design language

The palette and motifs are lifted straight from `zlang/src/main.rs`:

- **Cyan** (`#22d3ee`) — the CLI's bold-cyan accent for commands and headings.
- **Green / yellow / red** — success, warning, and error, matching `✔ ⚠ ✖`.
- **Rounded panels** — inspired by the CLI's `╭─╮ │ ╰─╯` box-drawing frames.
- **Block-letter Z** — the `█▀▀` wordmark, rendered as an SVG logo.

## Customizing

- Colors, fonts, and spacing live in the `:root` variables at the top of `styles.css`.
- The hero terminal animation's script is the `script` array in `script.js`.
