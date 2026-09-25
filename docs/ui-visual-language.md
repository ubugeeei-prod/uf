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

Motion is **present and quiet**. Every state change the eye should follow
moves, visibly and briefly, and nothing performs. The first defaults animated
`all` loosely. #1414 fixed that by cutting motion until it was nearly absent,
which was a different failure. The rules below are the middle.

- **Three durations, by how far the eye has to follow.**
  - `durationFast` (120ms) is for a change in place: a colour, a pressed
    button, a focus ring.
  - `durationBase` (200ms) is for something small that arrives or travels: a
    popover, a menu, a tooltip, a switch's thumb, a drawn check.
  - `durationSlow` (280ms) is for something large: a dialog, a sheet, a
    drawer, a toast, a progress fill.

  Under about 100ms a transition is not seen. Past about 300ms the interface
  starts to perform.
- **Three easings, none of which overshoots.**
  - `easing` is `cubic-bezier(0.2, 0, 0, 1)`, for moving between two resting
    places.
  - `easingEnter` is `cubic-bezier(0, 0, 0.2, 1)`, a decelerate for arriving.
    It is half-way at a fifth of the time, so the content is legible at once,
    and the settle is still seen.
  - `easingExit` is `cubic-bezier(0.4, 0, 1, 1)`, an accelerate for leaving.

  Nothing bounces, and no `scale()` goes below 0.9 or above 1.
- **What moves, and how:**

  | Part | Enter | Duration, easing |
  | --- | --- | --- |
  | Dialog, alert dialog | scrim fades in; panel fades in from `scale(0.96)` | slow, enter |
  | Sheet, drawer | scrim fades in; panel slides its own size in from `data-side` | slow, enter |
  | Popover, menu, select and combobox lists, hover card, tooltip, date pickers | fade in and travel 4px out of the trigger, from `data-side` | base, enter |
  | Navigation menu panel | fades in and drops 4px out of the bar | base, enter |
  | Toast | fades in and rises 16px into the stack | slow, enter |
  | Button | gives to `scale(0.97)` while pressed | fast, standard |
  | Focus ring | draws outward, `outline-width` 0 to 2px | fast, standard |
  | Switch | thumb slides, track colour turns with it | base, standard |
  | Checkbox | box fills; the tick is drawn from its short arm (`stroke-dashoffset`) | fast / base, standard |
  | Radio | ring colour, then the dot fades in | fast / base |
  | Tabs | underline and label colour change together | base, standard |
  | Progress | the fill's `scaleX` moves to the new value | slow, standard |
  | Chevrons (accordion, collapsible, select, navigation menu) | a half turn | base, standard |
| Menu item highlight | background follows the pointer | fast, standard |
| Field | border colour turns; focus ring draws outward | fast, standard |

- **The preset moves the same way.** `@uniflowed/stylex/preset` is the
  default look for a project that styles `@uniflowed/ui` without the
  registry, and it follows the table: `backdropStyles()` fades in,
  `dialogStyles()` fades in from `scale(0.96)` around its centring translate,
  `menuStyles()` travels 4px from `data-side`, and buttons, fields, menu
  items, tabs and controls transition their colours and focus rings.
- **Enter comes from a `@starting-style`.** uf's StyleX has no `@keyframes`.
  An entrance does not need them: a `@starting-style` value is the style an
  element is taken to have had before it was inserted, so the transition runs
  from it as soon as the part mounts. An anchored surface reads the side
  `@uniflowed/ui` put it on (`data-side`) and starts 4px back towards its
  trigger.
- **Exit is Planned.** Every overlay part in `@uniflowed/ui` unmounts the
  moment it closes, so there is nothing left to animate, and leaving is still
  a cut. Exit transitions need the behaviour layer to keep a closing part
  mounted, marked `data-state="closed"`, until its transitions finish. The
  same work gives accordion and collapsible height and a sliding tabs
  indicator. `easingExit` is declared for that work, which ubugeeei-prod/uf#1564
  tracks.
- **Named properties only.** A transition lists what it moves, never `all`,
  and a duration never appears without a property list. Nothing animates a
  layout property: the progress bar moves a `transform`, not its `width`. The
  one exception is the drawer, a fixed overlay that resizes between snap
  points without moving anything else. A slider's thumb follows the pointer
  exactly and does not ease.
- **Reduced motion fades or stops.** Under `prefers-reduced-motion: reduce`
  nothing travels, grows or is drawn. A transition either becomes `0s` or
  keeps only opacity and colour, so an overlay still fades in rather than
  cutting, and a sheet fades where it would have slid.
- `crates/uf_stylex/src/tests/defaults.rs` enforces each of these rules on the
  compiled styles and on the tokens. `what_deserves_motion_has_it` is the
  other half: it fails if an overlay loses its entrance or a control stops
  moving, so the defaults cannot go quiet again unnoticed. An overlay's
  entrance there means a `@starting-style` opacity, a `@starting-style` for
  what travels or grows, and `easingEnter`, in the registry and in the preset
  alike.

## Where this does not apply

`@uniflowed/brand` holds uf's logo identity, including the cyan-to-magenta
spectrum, and the documentation site uses it. The interface defaults take
brand's type and spacing scales, but none of its colours or corners.
