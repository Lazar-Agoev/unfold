# Dark theme: light -> dark mapping

Source: artboard **v2 · 13 · Board — dark** compared node by node with **v2 · 02 · Board — canvas** (same structure, same copy, same
geometry). Only colors and shadows change; no size, spacing, radius or type change except the active tab (see 3).

Paper dark tokens referenced below:

| Paper token | Hex | Code token today (dark) | Code value | Match? |
| --- | --- | --- | --- | --- |
| `--color-reader-dark-canvas` | `#151619` | `--color-canvas` | `#15161a` | ~ (1 step off) |
| `--color-reader-dark-surface` | `#1D1F23` | `--color-surface` | `#1c1d22` | ~ |
| `--color-reader-dark-sidebar` | `#191B1F` | `--color-sidebar` / `--color-chrome` | `#18191d` | ~ |
| `--color-reader-dark-text` | `#EDEEF2` | `--color-text` | `#e4e5ea` | **differs** |
| `--color-reader-dark-muted` | `#A4A8B3` | `--color-muted` | `#9a9ea9` | **differs** |
| `--color-reader-dark-border` | `#353840` | `--color-border` | `#2d2f36` | **differs** |
| `--color-reader-dark-accent` | `#A0AAFF` | `--color-accent` / `--color-accent-text` | `#5862c0` / `#949be9` | **differs: dark accent fill is the light periwinkle, with dark text on it** |
| `--color-reader-dark-selection` | `#2A304B` | `--color-selection` | `#262a45` | **differs** |
| `--color-reader-dark-control` | `#2D3036` | — | — | new (raised controls, monogram, active segments) |

The code's dark set is close but not equal. Adopt the Paper values (they are what the artboard was designed and checked with) and
re-run `ui/src/theme/contrast.test.ts`.

## 1. Element-by-element table

"untokened" = the hex matches no Paper token.

### Window and tab strip

| Element | Light | Dark |
| --- | --- | --- |
| Window root / canvas area fill | `#F7F8FA` reader-light-canvas | `#151619` reader-dark-canvas |
| Tab strip fill | `#ECEDF0` untokened | `#111214` untokened (darker than canvas) |
| Tab strip bottom border | `#E0E2E7` reader-light-border | `#2B2E34` untokened |
| Home icon / inactive tab label | `#656974` muted | `#A4A8B3` reader-dark-muted |
| Active tab fill | `#FFFFFF` | `#191B1F` reader-dark-sidebar |
| Active tab border (top/left/right) | `#E0E2E7` | `#2B2E34` untokened |
| Active tab label | `#202126` text | `#EDEEF2` reader-dark-text |
| Monogram fill | `#E6E8EC` untokened | `#2D3036` reader-dark-control |
| Monogram letter | `#3F4451` untokened | `#C9CCD4` untokened |
| Traffic lights | `#FF5F57` `#FEBC2E` `#28C840` | same |

### Sidebar

| Element | Light | Dark |
| --- | --- | --- |
| Sidebar fill | `#F7F8FA` reader-light-canvas | `#191B1F` reader-dark-sidebar |
| Sidebar right border | `#E0E2E7` | `#2B2E34` untokened |
| Selected-section top rule | `#E0E2E7` | `#2B2E34` untokened |
| Breadcrumb text, section labels, "follow order", relation labels, location, "2 Flow steps unlabeled ·" | `#656974` reader-light-muted | `#A4A8B3` reader-dark-muted |
| Icons at rest (back, `⋯`, `+`, collapse, page, entrance, group, ƒ glyph) | `#656974` | `#A4A8B3` reader-dark-muted |
| Board title, row labels, relation values, identity name | `#202126` reader-light-text | `#EDEEF2` reader-dark-text |
| Selected row fill | `#EDF0FC` reader-light-selection | `#2A304B` reader-dark-selection |
| Selected row icon / ƒ | `#5862C0` accent | `#A0AAFF` reader-dark-accent |
| "Label them", "+ Open" | `#5862C0` | `#A0AAFF` |
| Review dot | `#D97706` decision | `#F59E0B` untokened in Paper (= code `--color-pending` dark) |
| Description box fill | `#FBEFF5` reader-light-authored-tint | `#A8346F2E` (authored `#A8346F` at 18 %) |
| Description text + byline | `#A8346F` reader-light-authored | `#F2A9CC` untokened (byline still opacity .85) |

### Top bar

| Element | Light | Dark |
| --- | --- | --- |
| Top bar fill | `#FFFFFF` | `#191B1F` reader-dark-sidebar (**not** dark surface) |
| Top bar bottom border | `#E0E2E7` | `#2B2E34` untokened |
| Page title | `#202126` | `#EDEEF2` |
| Publish-state dot | `#5862C0` | `#A0AAFF` |
| Publish-state text | `#656974` | `#A4A8B3` |
| Share button fill | `#FFFFFF` | `#2D3036` reader-dark-control |
| Share button border | `#E0E2E7` reader-light-border | `#353840` reader-dark-border |
| Share label / chevron | `#202126` / `#656974` | `#EDEEF2` / `#A4A8B3` |
| Publish button fill | `#5862C0` | `#A0AAFF` reader-dark-accent |
| Publish button label | `#FFFFFF` | `#151619` reader-dark-canvas |
| Lens track | `#ECEDF0` untokened | `#111214` untokened |
| Lens active segment fill | `#FFFFFF` | `#2D3036` reader-dark-control |
| Lens active segment shadow | `0 1px 2px #0F172A14` | `0 1px 2px #00000066` |
| Lens active label / inactive label | `#202126` / `#656974` | `#EDEEF2` / `#A4A8B3` |

### Dock

| Element | Light | Dark |
| --- | --- | --- |
| Dock fill | `#FFFFFF` | `#1D1F23` reader-dark-surface |
| Dock border | `#E0E2E7` | `#353840` reader-dark-border |
| Dock shadow | `0 1px 2px #0F172A0F, 0 8px 24px #0F172A1A` | `0 1px 2px #00000066, 0 12px 32px #0000008C` (larger blur and offset) |
| Dividers | `#E0E2E7` | `#353840` |
| Add fill | `#5862C0` | `#A0AAFF` |
| Add icon + label | `#FFFFFF` | `#151619` reader-dark-canvas |
| Add key chip fill / letter | `#FFFFFF2E` / `#FFFFFF` | `#15161924` (canvas at 14 %) / `#151619` |
| Back icon (enabled) | `#656974` | `#A4A8B3` |
| Forward icon (disabled), trail chevron | `#94A3B8` faint | `#5F636D` untokened |
| Trail earlier step / current step | `#656974` / `#202126` | `#A4A8B3` / `#EDEEF2` |
| Arrange icon / label | `#656974` / `#202126` | `#A4A8B3` / `#EDEEF2` |
| Zoom −/+ icons, %, Fit | `#656974`, `#202126`, `#202126` | `#A4A8B3`, `#EDEEF2`, `#EDEEF2` |

### Canvas: cards and arrows (chrome-level)

| Element | Light | Dark |
| --- | --- | --- |
| Card fill | `#FFFFFF` | `#1D1F23` reader-dark-surface |
| Card border | `#E0E2E7` | `#353840` reader-dark-border |
| Card header divider | `#E0E2E7` | `#2E3138` untokened |
| Card description divider | `#F0F1F4` untokened | `#2A2C32` untokened |
| Selected card border (1.5px) | `#5862C0` | `#A0AAFF` |
| Selected card ring (3px) | `#5862C01F` (accent 12 %) | `#A0AAFF29` (dark accent 16 %) |
| Card title / location / ƒ (unselected) | `#202126` / `#656974` / `#656974` | `#EDEEF2` / `#A4A8B3` / `#A4A8B3` |
| ƒ on selected card | `#5862C0` | `#A0AAFF` |
| Card mini-lens track | `#F3F4F6` reader-light-sidebar | `#151619` reader-dark-canvas |
| Card mini-lens active segment | `#FFFFFF` | `#2D3036` reader-dark-control |
| Card description text, "more" | `#A8346F` | `#F2A9CC` untokened |
| Review banner fill | `#FFF3D6` reader-light-warn-fill | `#D9770629` (decision at 16 %) |
| Review banner text + "Review" | `#8A5A00` reader-light-warn | `#F5C66A` untokened |
| Highlighted call row (step) | `#F4F5F8` reader-light-step | `#24262C` untokened |
| Line numbers | `#64748B` subtle | `#8B8F99` untokened |
| Call link (underlined) | `#5862C0` | `#A0AAFF` |
| Dashed underline (unresolved call `release`) | `#8B93A1` contour | `#6B6F7A` untokened |
| Arrow to an opened call (stroke + head) | `#5862C0` | `#A0AAFFB3` (dark accent at 70 %) |
| Arrow entrance -> handler (contour) | `#8B93A1` | `#5F636D` untokened |
| Entrance card: method, location, caps label | `#656974` | `#A4A8B3` |
| Entrance card: handler names | `#202126` | `#EDEEF2` |
| Entrance card: opened handler | `#5862C0` | `#A0AAFF` |

### Syntax (inside cards, also the peek sheet)

| Role | Light (Paper token) | Dark (13) | Code dark today |
| --- | --- | --- | --- |
| Plain | `#202126` syntax-plain | `#EDEEF2` reader-dark-text | `#e4e5ea` |
| Keyword | `#0033B3` syntax-keyword | `#8FA8FF` | `#7f9cf5` |
| Function name | `#00627A` syntax-function | `#6CC5D8` | `#56b6c2` |
| Number | `#B4530A` syntax-number | `#F0A36B` | `#e0955a` |
| External call (`create`) | `#DB2777` external | `#D59BE8` | (no role) |
| String / type / property / comment | not visible in 13 | — | keep code values until designed |

The dark syntax colors in 13 differ from the code's dark syntax set; there are no Paper tokens for them.

### Flow nodes inside the reserveInventory card (for reference; card internals)

| Node | Light fill / border / text | Dark fill / border / text |
| --- | --- | --- |
| Input pill "items" | `#F1F5F9` / `#CBD5E1` / `#475569` | `#25282E` / `#3C4049` / `#C9CCD4` |
| Connector (1.5×12) | `#CBD5E1` | `#454953` |
| Decision "Every item in stock?" | `#FEF3C7` / `#F3C66B` / `#7A4B00` | `#33291A` / `#6E5524` / `#F5C66A` |
| YES branch box | `#EFF6FF` / `#93C5FD` / `#1E3A8A` | `#1B2638` / `#34507A` / `#AFCBF5` |
| NO branch box | `#FEE2E2` / `#FCA5A5` / `#991B1B` | `#36201F` / `#6E3434` / `#F4A9A9` |
| YES / NO labels | `#656974` | `#A4A8B3` |

## 2. Shadows

| Thing | Light | Dark |
| --- | --- | --- |
| Floating (dock; menus and sheets by extension) | `0 1px 2px rgb(15 23 42 / .06), 0 8px 24px rgb(15 23 42 / .10)` | `0 1px 2px rgb(0 0 0 / .40), 0 12px 32px rgb(0 0 0 / .55)` |
| Active segment | `0 1px 2px rgb(15 23 42 / .08)` | `0 1px 2px rgb(0 0 0 / .40)` |
| Selection ring | `0 0 0 3px rgb(88 98 192 / .12)` | `0 0 0 3px rgb(160 170 255 / .16)` |

Menus, popovers, the peek sheet, toasts and the placeholder card are **not drawn in dark**. Extrapolate: menus/sheet use the dock's
fill (`#1D1F23`, or code `--color-popover` lifted one step), border `#353840`, and the dark floating shadow.

## 3. Structural differences

These are not just color swaps; the implementer must pick one rule for both themes.

1. **Surface roles do not line up.**

   | Region | Light | Dark |
   | --- | --- | --- |
   | Sidebar | = canvas (`#F7F8FA`) | = sidebar token (`#191B1F`), **lighter** than canvas |
   | Top bar | = surface (`#FFFFFF`) | = sidebar token (`#191B1F`), **not** surface |
   | Active tab | = surface (`#FFFFFF`), matches top bar | = sidebar token, matches top bar and sidebar |
   | Canvas | `#F7F8FA` | `#151619` (darkest of the three) |

   Consistent reading: **the active tab always matches the top bar** (white in light, `#191B1F` in dark), and in dark the chrome
   (sidebar + top bar + active tab) is one tone above the canvas. In light the sidebar sits at canvas tone instead. Suggested
   tokens: `--color-chrome` (top bar + active tab: light `#FFFFFF`, dark `#191B1F`), `--color-sidebar` (light `#F7F8FA`, dark
   `#191B1F`), `--color-canvas` (light `#F7F8FA`, dark `#151619`).

2. **Two dark hairlines.** Shell edges (tab strip, sidebar edge, top bar, Selected rule) use `#2B2E34`; controls and cards (Share
   border, dock border, dividers, card border) use `#353840` reader-dark-border; card-internal dividers use `#2E3138`/`#2A2C32`. Light
   uses one hairline (`#E0E2E7`) for both shell and controls. Suggest `--color-border` (`#353840` dark) and a new
   `--color-border-shell` (`#E0E2E7` light / `#2B2E34` dark).

3. **Active tab merges in dark only.** Dark active tab is 31px tall with `margin-bottom: -1px; padding-bottom: 1px; z-index: 1`, so it
   covers the strip's bottom border and joins the top bar. Light active tab is 30px and the 1px strip border runs under it. Apply the
   merge in both themes.

4. **Accent fill flips to a light color.** Primary buttons (Publish, Add) are `#A0AAFF` with **dark** text `#151619` in dark, versus
   `#5862C0` with white text in light. Code today keeps `#5862C0` as the dark accent fill with white text; the design changes that.
   Add `--color-on-accent` dark = `#151619`.

5. **Contrast check for the darker tab strip.** Inactive tab labels are `#A4A8B3` on `#111214`; passes AA comfortably, but add the
   pair to `contrast.test.ts`.
