# The default UI's visual language

`uf ui add` copies components from `registry/ui/` into a project, styled with
the tokens in `packages/stylex/tokens.stylex.js` and the themes in
`packages/stylex/theme.js`. This note records what those defaults look like
and why, so a change to them starts from the idea rather than from a guess.

Only rules the tests enforce are listed. Where a test enforces a rule, it is
named.

## The idea: paper, ink and one pencil

The defaults should look like a well-set document, not a product launch. The
interface is **neutral first**. Near-black ink sits on off-white paper, and
surfaces are separated by thin rules. Colour is **spent in one place**: a
single accent marks what is selected, current or primary. Nothing glows,
floats or shimmers. A project that wants a personality supplies it with a
theme. The defaults leave room for one instead of competing with it.

This is a deliberate move away from the look most generated interfaces share:
violet and indigo accents, slate-blue greys, navy dark modes, soft shadows,
gradients and large radii.

## Colour

- **Greys have no tint.** Canvas, surfaces, rules, ink and muted text are warm
  neutrals, with at most a few steps between their red, green and blue
  channels. `the_neutrals_are_grey_rather_than_tinted` enforces this.
- **One accent, a quiet cobalt** (`#2b58b5` light, `#8eaee8` dark). Blue is
  the colour readers already take to mean "you can act on this", so it needs
  no explanation. It is darkened and desaturated so it reads as ink, not as
  light. The accent and the focus ring stay out of the indigo-to-magenta band.
  `the_accent_is_not_violet` enforces this.
- **Semantic colour is calm.** Danger is a brick red (`#b1271d` / `#ee8a80`),
  not a signal red. The soft variants are tints you notice without being
  alarmed by them.
- **Light and dark are two palettes.** Dark is charcoal (`#141413`,
  `#1c1c1a`), not navy. Its accent is lighter and less saturated, so it does
  not become neon on a dark ground. The recessed surface is darker than the
  canvas, as a well would be.
- **Contrast.** Every text/background pair the components use is at least
  4.5:1 in both modes, and the focus ring is at least 3:1 against the surfaces
  it is drawn on. `crates/uf_stylex/src/tests/preset.rs` computes these ratios
  from the compiled stylesheet.

## Shape and depth

- **Small corners, one scale.** `radiusSm` 2px for inner parts (menu items,
  options, badges, checkboxes), `radiusMd` 4px for controls (buttons, fields,
  triggers, popovers, menus), and `radiusLg` 6px for the largest surfaces
  (cards, dialogs, the page-facing edge of a sheet). `radiusPill` is only for
  things that are round by nature: a switch, a slider rail, a radio dot, a
  scrollbar thumb, an avatar.
- **No shadows and no gradients.** A surface in front of another has a 1px
  border and its own background. A focus ring is an `outline`.
- `crates/uf_stylex/src/tests/defaults.rs` compiles every default style and
  fails on a `box-shadow`, a `text-shadow`, a `drop-shadow()`, a gradient, or a
  corner that is not one of the tokens.

## Calendars

`registry/ui/calendar.js` is shared by the calendar, the range calendar and
both date pickers.

- **A grid that lines up.** Every day cell is 36px square, so the hit target
  is over 32px. Each weekday heading is one day wide, the table layout is
  fixed, and numerals are `tabular-nums`. The columns stay columns in every
  locale, including the ones whose short weekday names are whole words.
- **The month name sits between the month buttons.** It is the grid's
  `<caption>` and so its accessible name. The buttons hang on the caption's
  line at its inline ends, so a right-to-left page swaps them and mirrors
  their arrows.
- **States with little fill.** Today is the accent numeral in bold. The chosen
  day is the only filled cell. A range is two filled ends joined by a pale
  band, squared off where the ends meet the band. An unavailable day is muted
  and struck through. Hover is a neutral tint, and keyboard focus is an inset
  2px outline that stays visible on a filled day.
- The frame is 278px wide, which fits a 320px screen.

## Motion

- **Short, with one easing.** `durationFast` is 120ms, for a colour or a
  chevron. `durationBase` is 160ms, for something that travels, such as a
  switch thumb, a drawer or a progress bar. There is one easing,
  `cubic-bezier(0.2, 0, 0, 1)`. It decelerates and never overshoots, so
  nothing bounces.
- **Named properties only.** A transition lists what it moves, never `all`.
  Nothing scales up from nothing: the radio dot fades in instead of popping.
  Nothing animates a layout property: the progress bar moves a `transform`,
  not its `width`. The one exception is the drawer, a fixed overlay that
  resizes between snap points without moving anything else.
- **Reduced motion is still.** Every transition is `0s` under
  `prefers-reduced-motion: reduce`.
- `crates/uf_stylex/src/tests/defaults.rs` enforces each of these rules on the
  compiled styles and on the tokens.

## Where this does not apply

`@uniflowed/brand` holds uf's logo identity, including the cyan-to-magenta
spectrum, and the documentation site uses it. The interface defaults take
brand's type and spacing scales, but none of its colours or corners.
