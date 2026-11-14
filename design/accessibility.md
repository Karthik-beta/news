# Accessibility

Target: **WCAG 2.2 level AA** on every page, in both themes, at every width from 320px up. These rules are part of the design, not a later check.

## Colour and contrast

| Requirement | Rule here |
|---|---|
| Body and small text | ≥ 4.5:1 against its background. All ink tokens pass on both paper and the note wash; see the contrast table in [color.md](color.md#contrast). |
| Large text (≥ 24px, or ≥ 18.5px bold) | ≥ 3:1. |
| Chart marks, icons, focus rings, input borders | ≥ 3:1 against their surface, or the value is also shown as visible text (direct labels or a table). |
| Meaning | Never colour alone. Changes are written ("Up 2.0%", "−5.7%"), series are directly labelled, the highlighted bar also has a bold label, primary and context sources have text badges. |
| Colour-blind safety | Chart palettes are validated by script for protanopia and deuteranopia (OKLab ΔE ≥ 8 between neighbouring series). See [color.md](color.md#chart-colours). |
| Forced colours / Windows high contrast | Charts keep their shapes and labels; bars use `forced-color-adjust: none` only for the data fill, and every mark has a text equivalent. |

## Charts

Every chart ships with:

1. A **takeaway title** and a **subtitle** with the unit, as visible text (not baked into an image).
2. An **accessible name** that states the data in a sentence (all values when there are 12 or fewer points).
3. A **"Show the data table"** toggle that reveals a real `<table>` with the same numbers, and a **CSV** link.
4. **Keyboard access** to tooltips: every interactive mark (or a single focusable chart region with arrow-key stepping) shows its tooltip on focus as well as hover.
5. A **source line** as text.

Charts are drawn as SVG or HTML, never as raster images, so they scale and stay readable when zoomed to 200%.

## Structure

- One `h1` per page (the headline). Section headings are `h2`; chart titles are `h2` inside `figure`/`figcaption`.
- Landmarks: `header` (masthead), `nav` (sections, region filter), `main`, `aside` (rail), `footer`.
- Fly titles are a paragraph before the `h1`, not part of the heading, so screen readers announce the headline first in the heading list.
- Key figures are a list (`dl` or `ul`) of label, value and change.
- Data tables use `th scope`, a `caption` (or `aria-labelledby` to the chart title), and right-aligned numeric columns.
- The sources panel is a list; each item's type and licence are text, not icons.

## Keyboard and focus

- Everything interactive is reachable and usable by keyboard, in reading order.
- Focus ring: 2px solid `color.focus`, 2px offset, on every interactive element. Never removed.
- A "Skip to content" link is the first focusable element; it becomes visible on focus.
- Tooltips and glossary definitions open on focus and close on Escape.
- Click-to-load video placeholders are buttons with a clear label ("Load video from YouTube: Federal Reserve press conference, 18 September 2024").

## Touch targets

- Minimum 24×24px for any target (WCAG 2.2 2.5.8); aim for 44×44px for navigation, buttons and filter controls on phone.
- Tapping a key figure or chart mark shows its source; tapping outside closes it.

## Motion

- No auto-playing media and no animation on load.
- Transitions ≤ 200ms, only in response to the reader. `prefers-reduced-motion: reduce` removes them.

## Text and zoom

- Text resizes to 200% without loss of content; layouts reflow at 320px CSS width without sideways scrolling (tables and wide charts scroll inside their own container).
- No text in images. The wordmark is live text.
- Line height ≥ 1.5 for body text; paragraph spacing ≥ 0.8em.

## Images and video

- Every image has alt text (see [content-style.md](content-style.md#alt-text-and-accessible-summaries)) and a visible credit and licence.
- If an image fails to load, a neutral placeholder shows the alt text and credit at the same size, so the layout never shifts.
- YouTube embeds load only on click (`youtube-nocookie.com`), and the placeholder says the video will load from YouTube.

## Forms

- Every input has a visible label; required fields say "required" in the label.
- Errors appear in text next to the field and in a summary at the top, linked to the field, and are announced (`aria-live="polite"`).
- The error-report form works without JavaScript.

## Testing

- Automated: axe-core (or equivalent) on every rendered page in the quality checks, both themes. Any serious or critical violation fails the page's check.
- Contrast and palette checks run when tokens change (`validate_palette.js` for chart colours, a text-contrast script for ink tokens).
- Manual, per component, before the library ships: keyboard only, VoiceOver (iOS Safari), TalkBack (Android Chrome), NVDA (Windows Firefox), 200% zoom, Windows high-contrast mode.
