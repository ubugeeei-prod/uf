#![cfg_attr(test, allow(clippy::disallowed_macros))]

//! `uf ui`: styled components a project owns, written into it by one command.
//!
//! `@uniflowed/ui` is headless: keyboard maps, ARIA contracts, composition
//! types, and no styles. That is React Aria's half of a design system. The half
//! people reach for shadcn/ui for is a different thing — a styled component *in
//! their own repository*, which they read and change — and `uf ui add` is that
//! half. `uf ui add dialog` writes `app/components/ui/dialog.js`, a dialog built
//! on `@uniflowed/ui` and styled with `@uniflowed/stylex`'s tokens, and
//! from then on the file is the project's. ubugeeei-prod/uf#947 asked for it.
//!
//! This crate is the registry and the decisions a reader has to be able to
//! trust: what a component is, what a copy remembers about where it came from,
//! and when a file that already exists may be replaced.
//! `crates/uf_cli/src/commands/ui.rs` is the command, and it renders what this
//! crate decides.
//!
//! # A copy step, after ubugeeei-prod/uf#303 declined one
//!
//! `npm/ui/index.js` records why `@uniflowed/ui` has no copy step: a copied
//! focus trap is a fork nobody's upgrade reaches, and a `renders*` constraint
//! means nothing once the source has been pasted into an application. Both are
//! arguments about copying *behaviour*, and neither is reopened here, because
//! nothing here copies behaviour. A registry component imports its parts from
//! `@uniflowed/ui` and owns only what a person changes when they say "our
//! dialog": the scrim, the spacing, which tone the trigger wears. A focus-trap
//! fix still reaches a project that ran `uf ui add dialog` a year ago, the day it
//! upgrades `@uniflowed/ui`. And the composition constraints survive the copy:
//! the copied `Tabs.Tab` declares `renders` `@uniflowed/ui`'s `Tabs.Tab`, and the
//! copied `Tabs.List` takes `renders*` the copied one, so a `<button>` dropped
//! into the copied tab list is still a Flow error, in the project's own file.
//!
//! # Where the registry lives: in this repository, inside the binary
//!
//! `npm/ui/registry/` at the root of this repository, embedded into `uf` when it is
//! built. So `uf ui add` works offline, answers at once, and always writes the
//! component that matches the uf running it — the `@uniflowed/*` version it
//! pins, the StyleX conditions its compiler accepts, the parts `@uniflowed/ui`
//! exports at that version. A registry fetched over the network can be newer or
//! older than the compiler that has to compile what it serves, and "works with
//! the uf you have" is the property a component a project owns needs most. The
//! cost is that a fix to a registry component ships with a uf release, which is
//! the cadence the packages it is built on move at anyway.
//!
//! The copyable components live beside the headless implementation in
//! `npm/ui/registry/`. The binary embeds them, the documentation renders them,
//! and the repository's formatter, checker and test runner validate that same
//! source. `npm/ui` owns both surfaces; the headless API is its package root,
//! while `uf ui add` copies the styled files into the consuming project.
//!
//! # A component is one file, and the registry is one directory
//!
//! `npm/ui/registry/<name>.js` is the component, and it is the file `uf ui add`
//! writes — byte for byte, with one line appended ([`stamp`]). Beside it are
//! `<name>.example.js`, which the reference renders live, and `<name>.test.js`,
//! which this repository's suite runs. [`registry::EMBEDDED`] is the list, and a
//! test holds it to the directory in both directions.
//!
//! One directory rather than a directory per component, which is what #947
//! sketched, and the reason is imports. A dialog imports `./button.js`. In a
//! project both files sit in `app/components/ui/`, so that resolves; in a
//! registry of one directory per component it would not, and each way out was
//! worse than giving the directories up. Rewriting imports as a file is written
//! — shadcn's aliases — makes the file this repository tests a different file
//! from the one a project receives, with every rewrite rule a new way for the two
//! to differ. Laying the registry out the way a project is laid out keeps them
//! the same bytes: the import graph `uf check` and `uf test` walk here is the one
//! a project walks, and nothing is rewritten.
//!
//! # What a component declares, and what is read from it
//!
//! Nothing is written down twice. The source is the manifest:
//!
//! * **Its npm dependencies** are the packages it imports —
//!   `@uniflowed/stylex/tokens.stylex.js` is `@uniflowed/stylex` — and an
//!   `@uniflowed/*` package is added at this uf's
//!   own version, for the reason `uf_project`'s template pins it: the packages
//!   and the binary are two halves of one release.
//! * **The components it needs** are the siblings it imports. `./button.js` is
//!   `button`, so `uf ui add dialog` writes `button.js` as well.
//! * **Its description** is the first paragraph of its header, after the
//!   title: `// Dialog: …`.
//! * **Its accessibility notes** are the rest of the header, under "What to keep
//!   true when you change it". In the file, because the person who most needs
//!   them is the one editing the copy, and a note kept in a registry they never
//!   open is a note they never read.
//!
//! A list of dependencies kept beside the imports would be a list somebody
//! forgets to update. Reading the imports cannot disagree with them.
//!
//! # Where a component is written, and how its imports resolve
//!
//! Into [`DEFAULT_DIRECTORY`], or the directory a project's `ui.directory` names,
//! every component beside every other, so `./button.js` means in the project
//! exactly what it means in the registry, and a page imports
//! `./components/ui/dialog.js`. The CLI reads the key and refuses a path that
//! leaves the project; this crate is handed the directory.
//!
//! # Theming, dark mode included
//!
//! Every colour, space, radius, type size and duration a component uses is a
//! `ufTokens` variable from `@uniflowed/stylex/tokens.stylex.js`; a literal is
//! only ever geometry one control alone uses. So `ufAutoTheme` — dark mode that
//! follows the reader's system — `ufDarkTheme`, and a project's own
//! `stylex.createTheme(ufTokens, …)` restyle every component without an edit to
//! any of them, and the text colours sit on the backgrounds
//! `crates/uf_stylex/src/tests/preset.rs` measures at 4.5:1 in both shipped
//! themes. State is drawn from the attributes a part already writes —
//! `:is([aria-selected=true])`, `:is([data-active=true])` — rather than tracked
//! a second time beside the part that owns it.
//!
//! # What a copy remembers about where it came from
//!
//! One line, appended to the file:
//!
//! ```text
//! // Written by `uf ui add dialog` from uf 0.0.0-alpha.33, sha256 1f3a….
//! ```
//!
//! The version says which registry the copy began as, and the digest is of what
//! was written. Together they are how [`project::inspect`] tells a copy nobody
//! touched from one somebody edited, and a registry that has moved since from
//! one that has not — which is what `uf ui diff` needs to say whether a
//! difference is the project's or the registry's.
//!
//! In the file rather than in a lockfile beside the components. A lockfile is
//! one more file every branch that adds a component edits, and so a merge
//! conflict waiting for the second of two branches; a line in each file travels
//! with that file, through a rename, a move and a copy into another project, and
//! it is committed by definition, because it is part of what is committed.
//!
//! # A file that is already there
//!
//! [`project::plan_add`] decides, for every file a run would write, before any
//! is written. A copy nobody edited is brought up to this registry's version,
//! because nothing of anybody's is lost by it. A file somebody edited, or one
//! `uf ui add` did not write, is refused by name when the component was asked
//! for, with `uf ui diff` to see why and `--overwrite` to replace it anyway —
//! and left alone when it was only needed by a component that was asked for,
//! because an edited `button.js` still satisfies `./button.js`. A refusal is
//! found before anything is written or installed, so a run that stops has
//! changed nothing.

pub mod diff;
pub mod project;
pub mod registry;
pub mod stamp;
pub mod update;

pub use project::DEFAULT_DIRECTORY;
pub use registry::{Component, REGISTRY_VERSION, Registry, RegistryError, UnknownComponent};
pub use stamp::Stamp;

#[cfg(test)]
mod tests;
