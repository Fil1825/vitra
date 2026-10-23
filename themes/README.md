# Example themes

Themes to try and to start your own from. Each folder is a theme's source: `theme.json`,
`custom.css` and `assets/`. [docs/THEMES.md](../docs/THEMES.md) explains every part.

| Theme         | What it shows                                                                                                          |
| ------------- | ---------------------------------------------------------------------------------------------------------------------- |
| **Dunes**     | A light theme with a background picture, its own font (Manrope) and a layout with the big artwork under the sidebar.   |
| **Neon Grid** | A dark theme with strong glass and refraction, Exo 2, glowing edges and titles from `custom.css`, the controls on top. |
| **Paper**     | A quiet, almost opaque light theme: a tiled paper texture, no moving artwork, titles and lyrics in Noto Serif.         |

To install one, pack it and drop the file on Vitra's window (or Settings → Appearance →
**Import Theme…**):

```sh
cargo run -p themes --example pack -- themes/dunes   # → target/themes/dunes.vitratheme
```

The pictures were made for Vitra and are covered by the repository's license. The fonts are
under the SIL Open Font License 1.1 (`OFL.txt` in each folder; the license is also inside
each font file), cut down to Latin and Cyrillic.
