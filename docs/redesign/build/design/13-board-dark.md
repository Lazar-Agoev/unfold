# v2 · 13 · Board — dark

The same screen as [02 · Board — canvas](02-board-canvas.md) in the dark theme: same layer tree, same copy, same geometry, same
states (selected page, selected layer, selected card, review dot, review banner, "Published · 2 unpublished changes", Code lens,
Forward disabled). Only colors and shadows change. The full color table is in [dark-theme.md](dark-theme.md); this file lists what
is specific to the screen.

Artboard: 1440×900, background `--color-reader-dark-canvas` `#151619`.

## 1. Regions

Identical to [02 §1](02-board-canvas.md#1-regions-window-coordinates), with one exception:

| Region | Light (02) | Dark (13) |
| --- | --- | --- |
| Active tab "Order lifecycle" | 133 × 30, y 7–37, strip border visible beneath | 133 × **31**, `margin-bottom: -1px; padding-bottom: 1px; z-index: 1` — covers the strip's bottom border and joins the top bar |

## 2. Region fills at a glance

| Region | Fill | Edge |
| --- | --- | --- |
| Tab strip | `#111214` | bottom `#2B2E34` |
| Active tab | `#191B1F` (reader-dark-sidebar) | top/left/right `#2B2E34` |
| Sidebar | `#191B1F` | right `#2B2E34`; Selected top rule `#2B2E34` |
| Top bar | `#191B1F` | bottom `#2B2E34` |
| Canvas / stage | `#151619` | — |
| Cards | `#1D1F23` (reader-dark-surface) | `#353840`; header divider `#2E3138` |
| Dock | `#1D1F23` | `#353840`; shadow `0 1px 2px #00000066, 0 12px 32px #0000008C` |

So in dark the chrome (tab, top bar, sidebar) is one tone, the canvas is darker, and cards and the dock are lighter than both.

## 3. Copy

Identical to 02 (see [02 §2](02-board-canvas.md#2-copy-verbatim)).

## 4. Screen-specific notes

- **Primary fills invert**: Publish and Add are `#A0AAFF` with `#151619` text and icon; Add's key chip is `#15161924` with a `#151619`
  "A".
- **Arrows to opened calls** are the dark accent at 70 % (`#A0AAFFB3`) for stroke and head, so they read quieter than the selected
  card's full-strength `#A0AAFF` border. The entrance -> handler arrow is `#5F636D`.
- **Selected card ring** is `#A0AAFF29` (16 %), stronger than light's 12 % to show on dark.
- **Description** (sidebar box and card strip) is `#F2A9CC` on `#A8346F2E`.
- **Review**: dot `#F59E0B`; card banner `#D9770629` fill with `#F5C66A` text.
- **Code**: highlighted call rows `#24262C`; line numbers `#8B8F99`; keyword `#8FA8FF`, function `#6CC5D8`, number `#F0A36B`, external
  `#D59BE8`, plain `#EDEEF2`; links `#A0AAFF`; dashed unresolved underline `#6B6F7A`.
- **Card mini lens**: track `#151619` (canvas), active segment `#2D3036`.
- **Window lens**: track `#111214`, active segment `#2D3036` with `0 1px 2px #00000066`.
- **Share**: `#2D3036` fill with `#353840` border (light Share is white with a border).
- **Monograms**: `#2D3036` with `#C9CCD4` letter.

## 5. Not drawn in dark

Peek sheet, menus, link labels, placeholder card, toasts, the Selected section for a link, Home. Use the rules in
[dark-theme.md §2–3](dark-theme.md#2-shadows) to extrapolate.
