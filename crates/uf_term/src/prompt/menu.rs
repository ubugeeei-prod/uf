//! The menu's state: what is showing, what is highlighted, and why.
//!
//! Every function here is pure. The terminal is somebody else's problem — this
//! decides what a frame should contain given what has been typed, and it is
//! the part worth testing, because filtering and scrolling are where a picker
//! is wrong in ways a screenshot does not show.

use crate::text::display_width;

/// One thing a reader can choose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice<'a> {
    /// What running it looks like: `build`, `run test:lib`.
    pub name: &'a str,
    /// One line saying what it does.
    pub about: &'a str,
    /// The heading this belongs under, or `""` for none.
    pub group: &'a str,
}

impl<'a> Choice<'a> {
    /// A choice with a name and a description.
    pub const fn new(name: &'a str, about: &'a str) -> Self {
        Self {
            name,
            about,
            group: "",
        }
    }

    /// The same, under a heading.
    pub const fn grouped(name: &'a str, about: &'a str, group: &'a str) -> Self {
        Self { name, about, group }
    }
}

/// How many rows of the list are on screen at once.
///
/// Ten because a menu taller than that scrolls off a small terminal and stops
/// being a menu, and because a list you cannot see the end of is one you filter
/// rather than scroll.
pub const VISIBLE: usize = 10;

/// What the reader has done so far.
#[derive(Debug, Clone)]
pub struct Menu<'a> {
    /// Everything on offer, in the order it was given.
    choices: &'a [Choice<'a>],
    /// What has been typed.
    filter: String,
    /// Index into [`Self::matches`], not into `choices`.
    cursor: usize,
    /// First visible row, also an index into [`Self::matches`].
    offset: usize,
    /// Indices into `choices`, in the order they should be shown.
    matches: Vec<usize>,
}

impl<'a> Menu<'a> {
    /// A menu showing everything, with the first item highlighted.
    pub fn new(choices: &'a [Choice<'a>]) -> Self {
        let mut menu = Self {
            choices,
            filter: String::new(),
            cursor: 0,
            offset: 0,
            matches: Vec::with_capacity(choices.len()),
        };
        menu.refilter();
        menu
    }

    /// What has been typed so far.
    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// Whether anything matches what has been typed.
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }

    /// The highlighted choice, if there is one.
    pub fn selected(&self) -> Option<&'a Choice<'a>> {
        self.matches.get(self.cursor).map(|at| &self.choices[*at])
    }

    /// The rows to draw, top to bottom, each with whether it is highlighted.
    pub fn visible(&self) -> impl Iterator<Item = (&'a Choice<'a>, bool)> + '_ {
        self.matches
            .iter()
            .enumerate()
            .skip(self.offset)
            .take(VISIBLE)
            .map(move |(at, index)| (&self.choices[*index], at == self.cursor))
    }

    /// How many matches there are, and how many are off screen below.
    pub fn hidden_below(&self) -> usize {
        self.matches.len().saturating_sub(self.offset + VISIBLE)
    }

    /// How many matches are off screen above.
    pub fn hidden_above(&self) -> usize {
        self.offset
    }

    /// Move the highlight up, wrapping to the bottom.
    ///
    /// Wrapping rather than stopping: the item a reader wants is as often the
    /// last as the first, and pressing up once to reach it is what every menu
    /// with a `↑` on screen does.
    pub fn up(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.cursor = match self.cursor {
            0 => self.matches.len() - 1,
            at => at - 1,
        };
        self.scroll_to_cursor();
    }

    /// Move the highlight down, wrapping to the top.
    pub fn down(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.cursor = (self.cursor + 1) % self.matches.len();
        self.scroll_to_cursor();
    }

    /// Add a character to the filter.
    pub fn push(&mut self, character: char) {
        self.filter.push(character);
        self.refilter();
    }

    /// Remove the last character of the filter.
    pub fn backspace(&mut self) {
        self.filter.pop();
        self.refilter();
    }

    /// Empty the filter.
    pub fn clear(&mut self) {
        self.filter.clear();
        self.refilter();
    }

    /// The widest name among the matches, for the description column.
    ///
    /// Measured over what is *visible* rather than over everything: a filter
    /// that leaves three short names should not indent them past a long name
    /// that is no longer on screen.
    pub fn name_width(&self) -> usize {
        self.visible()
            .map(|(choice, _)| display_width(choice.name))
            .max()
            .unwrap_or(0)
    }

    /// Recompute the matches, and keep the highlight somewhere sensible.
    ///
    /// The highlight goes back to the top on every keystroke, which is what a
    /// reader typing a filter expects: the best match is first, and leaving the
    /// highlight where it was would select whatever happened to fall under it.
    fn refilter(&mut self) {
        self.matches.clear();
        let mut scored: Vec<(u32, usize)> = self
            .choices
            .iter()
            .enumerate()
            .filter_map(|(at, choice)| score(choice, &self.filter).map(|rank| (rank, at)))
            .collect();
        // Stable by rank, then by the order they were declared: a menu whose
        // items move around between keystrokes is one a reader stops trusting.
        scored.sort_by_key(|(rank, at)| (*rank, *at));
        self.matches.extend(scored.into_iter().map(|(_, at)| at));
        self.cursor = 0;
        self.offset = 0;
    }

    /// Scroll the window so the highlight is inside it.
    fn scroll_to_cursor(&mut self) {
        if self.cursor < self.offset {
            self.offset = self.cursor;
        } else if self.cursor >= self.offset + VISIBLE {
            self.offset = self.cursor + 1 - VISIBLE;
        }
    }
}

/// How well `choice` matches `filter`, lower being better; `None` for no match.
///
/// Four tiers, and they are in the order a reader means them. A name that
/// *starts* with what was typed is what they were reaching for; a name that
/// merely contains it is a near miss; a description match is a guess at what
/// they meant; and a subsequence match — `bld` for `build` — is the last
/// resort, because it matches almost everything if allowed to rank higher.
fn score(choice: &Choice<'_>, filter: &str) -> Option<u32> {
    if filter.is_empty() {
        return Some(0);
    }
    let filter = filter.to_lowercase();
    let name = choice.name.to_lowercase();

    if name.starts_with(&filter) {
        return Some(0);
    }
    if name.contains(&filter) {
        return Some(1);
    }
    if choice.about.to_lowercase().contains(&filter) {
        return Some(2);
    }
    is_subsequence(&name, &filter).then_some(3)
}

/// Whether every character of `needle` appears in `haystack`, in order.
fn is_subsequence(haystack: &str, needle: &str) -> bool {
    let mut wanted = needle.chars();
    let mut next = wanted.next();
    for character in haystack.chars() {
        if Some(character) == next {
            next = wanted.next();
            if next.is_none() {
                return true;
            }
        }
    }
    next.is_none()
}
