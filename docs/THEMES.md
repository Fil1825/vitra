# Vitra themes

A theme changes how Vitra looks: colors, the glass, fonts, corners, spacing, the window's
background, motion, and optionally the layout of the window. It is data, never code: Vitra
turns it into CSS custom properties and a cleaned style sheet. Themes can't run JavaScript or
load anything from the internet.

Three example themes live in [`/themes`](../themes): **Dunes** (light, a background picture,
its own font and a layout), **Neon Grid** (dark, glowing `custom.css`, controls on top) and
**Paper** (light and quiet, a tiled texture, serif titles).

## Making a theme

The easiest way is inside Vitra: Settings → Appearance → **New Theme** copies the theme in use
and opens the editor over the window, so the whole app previews every change. **Save** writes
it; **Export** in a theme's menu (⋯ or right-click) saves it as a `.vitratheme` to share.
Import one by dropping it on the window or with **Import Theme…** under the themes.

By hand: make a folder like the examples and pack it.

```
my-theme/
  theme.json        required
  custom.css        optional
  assets/           optional: pictures and fonts the theme uses
    sky.jpg
    my-font.woff2
```

```sh
cargo run -p themes --example pack -- path/to/my-theme     # → target/themes/my-theme.vitratheme
cargo run -p themes --example pack -- --out dist themes/*/ # several at once
```

A `.vitratheme` is a plain zip of those files (the folder itself may be the zip's single
top folder). Anything else in it, a readme or a license, is ignored.

## `theme.json`

[`docs/theme.schema.json`](theme.schema.json) is the full JSON Schema; it is generated from
`src/theme/schema.ts`, which Vitra checks every theme against. Point your editor at it for
completion and checks as you type:

```json
{
  "$schema": "https://raw.githubusercontent.com/fil1825/vitra/main/docs/theme.schema.json",
  "schemaVersion": 1,
  ...
}
```

In short:

| Field               | What it sets                                                                            |
| ------------------- | --------------------------------------------------------------------------------------- |
| `schemaVersion`     | Always `1`.                                                                             |
| `id`                | Lowercase letters, digits and hyphens; also the installed folder's name.                |
| `name`, `author`    | Shown in the theme picker.                                                              |
| `appearance`        | `light` or `dark`: the window chrome, scrollbars and system controls follow.            |
| `colors`            | Background, three text levels, accent and the text on it, separator, danger.            |
| `glass`             | Blur (px), saturation, tint and its opacity, border, rim highlight, refraction, shadow. |
| `typography`        | `fontFamily` (a CSS font stack), `fontSize` (11–20 px), `fonts` the theme brings.       |
| `radius`            | `small`, `medium`, `large` corner radii in px.                                          |
| `spacing.unit`      | The base of every gap and padding (2–8 px).                                             |
| `background`        | The window's backdrop: base opacity, gradient colors, the playing artwork, a picture.   |
| `accentFromArtwork` | Take the accent from the playing song's cover.                                          |
| `motion`            | Transition duration, easing (`standard`, `emphasized`, `linear`), backdrop speed.       |
| `layout`            | Optional: a layout of the window the theme suggests (see below).                        |

Colors are hex: `#RGB`, `#RGBA`, `#RRGGBB` or `#RRGGBBAA`. Refraction only shows where the
engine can render it (Windows); macOS and Linux get blur and the rim highlight.

### Fonts and pictures

Assets live in `assets/` and are named by `assets/<name>`: a letter or digit, then letters,
digits, `.`, `_` or `-`, and an extension.

- Pictures: `png`, `jpg`/`jpeg`, `webp`, `gif` (no SVG: it can carry scripts). Use one as the
  window's background with `background.image`: `{ "src": "assets/sky.jpg", "opacity": 0.8,
"fit": "cover" }` (`contain` fits it whole, `tile` repeats it at its own size).
- Fonts: `woff2` (best), `woff`, `ttf`, `otf`. Declare each file and name it in the stack:

  ```json
  "typography": {
    "fontFamily": "'Manrope', system-ui, sans-serif",
    "fontSize": 14,
    "fonts": [{ "family": "Manrope", "src": "assets/manrope.woff2", "weight": "200 800" }]
  }
  ```

  `weight` is one weight (`400`) or a range for a variable font (`100 900`); `style` is
  `normal` or `italic`. A font that only `custom.css` uses (titles, say) still goes in
  `fonts`. Only ship fonts whose license allows it (the examples use SIL OFL fonts, cut down
  to Latin and Cyrillic).

Limits: 32 MB per package, 16 MB per file, 64 assets, 512 KB for `theme.json` and for
`custom.css`.

### A layout

`layout` places the window's modules (the same structure Settings → Layout saves):

```json
"layout": {
  "zones": {
    "left": ["sidebar", "artwork"],
    "right": ["queue", "lyrics"],
    "top": [],
    "bottom": ["controls", "level"]
  },
  "hidden": ["queue", "lyrics", "level"],
  "widths": { "left": 260, "right": 320 },
  "weights": { "artwork": 0.8 },
  "sidebar": "expanded"
}
```

Modules: `sidebar`, `queue`, `lyrics`, `artwork`, `controls`, `level`. The columns (`left`,
`right`) take the sidebar, queue, lyrics and artwork; the bars (`top`, `bottom`) take the
controls; the level meter goes anywhere. `hidden` modules keep their place for when they are
shown. Widths are 180–560 px; `weights` share a column's height (0.25–4, 1 when left out);
`sidebar` is `expanded` or `rail` (icons only). Vitra asks before using a theme's layout; it
is also in the theme's menu (**Use Its Layout**).

## `custom.css`

For what the fields above can't say. It applies to the main window and the mini player.

Rules:

- `url()` may only name the theme's own assets (`url(assets/grain.png)`); anything else is
  replaced by `none`.
- `@import`, `@charset`, `@namespace`, `image-set()`, `expression()`, `behavior`,
  `-moz-binding` and `javascript:` are removed.
- What was removed is shown when the theme is imported or edited.

Vitra's class names change between versions; style these stable hooks instead:

| Selector                           | What it is                                                                                                                       |
| ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `[data-part='sidebar']`            | The sidebar module (likewise `queue`, `lyrics`, `artwork`, `controls`, `level`). Its box has the corners of the glass inside it. |
| `[data-part='center']`             | The title bar and the main view together.                                                                                        |
| `[data-part='header']`             | The title bar (history, open, settings buttons).                                                                                 |
| `[data-part='main']`               | The main view: library pages, Now Playing. Page titles are `h2`.                                                                 |
| `[data-part='lyrics-text']`        | Lyrics wherever they show (Now Playing, the lyrics module).                                                                      |
| `[data-part='settings']`           | The Settings panel.                                                                                                              |
| `[data-part='mini']`               | The mini player's glass (its window only).                                                                                       |
| `[data-part='backdrop']`           | The moving background behind everything.                                                                                         |
| `html[data-appearance='dark']`     | The theme's appearance (`light` / `dark`).                                                                                       |
| `html[data-platform='macos']`      | `macos`, `windows` or `linux`.                                                                                                   |
| `html[data-motion='reduce']`       | Reduce Motion or power saving is on: keep things still.                                                                          |
| `html[data-transparency='reduce']` | Reduce Transparency is on.                                                                                                       |

Every value the theme sets is a variable too: `--vitra-color-*` (`background`, `text`,
`text-secondary`, `text-tertiary`, `accent`, `accent-contrast`, `separator`, `danger`),
`--vitra-glass-*`, `--vitra-radius-sm|md|lg`, `--vitra-space-1` … `--vitra-space-12`,
`--vitra-font-family`, `--vitra-font-size`, `--vitra-motion-duration`, `--vitra-motion-easing`.
Set `--glass-radius` on a part to round its glass differently:

```css
/* Pill-shaped controls. */
[data-part='controls'] {
  --glass-radius: 999px;
}

/* Glowing edges. */
[data-part='controls'],
[data-part='sidebar'] {
  box-shadow: 0 0 28px #ff3ea533;
}

[data-part='main'] h2 {
  text-transform: uppercase;
  letter-spacing: 0.06em;
}
```

Two things to avoid: animating or lowering `opacity`, `filter` or `mask` on anything that
contains glass (the glass then stops seeing what is behind it, see the Gotchas in
`CLAUDE.md`), and heavy effects on large areas (WebKit repaints them on every frame of the
moving background).

## How Vitra treats a theme it imports

- The package is read with every size capped, and only `theme.json`, `custom.css` and
  well-named pictures and fonts are taken; a name that could escape the theme's folder
  rejects the package.
- `theme.json` must match the schema; the error says which field is wrong.
- A theme with the id of a built-in theme gets another id; one with the id of an installed
  theme replaces it or is kept beside it, as you choose.
- Installed themes live in Vitra's data folder, `themes/<id>/`, unpacked; `theme://` serves
  their assets to the window and nothing else.
