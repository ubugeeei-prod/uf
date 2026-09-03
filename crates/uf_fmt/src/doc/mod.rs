//! The document IR: what a printer *means*, before anyone decides where the
//! lines go.
//!
//! This is Wadler's "prettier printer" as Prettier ships it. A node printer
//! never measures a line; it says which pieces belong together ([`group`]),
//! where a line *may* break ([`LINE`], [`SOFTLINE`]) and where it *must*
//! ([`HARDLINE`]), and what to print differently when a group does break
//! ([`if_break`]). [`printer`] then makes one left-to-right pass deciding,
//! group by group, whether the flat form fits in the remaining width. Keeping
//! those two concerns apart is what makes Prettier's output reproducible from
//! Rust: the layout rules are all in the docs the node printers build, and
//! the printer is a fixed algorithm.
//!
//! Docs live in a bump arena and are handed around as `&'a DocNode<'a>`, so
//! building one never clones a subtree and a [`Doc`] is a `Copy` pointer.
//! Every node carries [`DocNode::breaks`], computed when it is built, which
//! is Prettier's `propagateBreaks` done eagerly: a group whose contents hold
//! a hard line is a broken group, and its parents are too.

pub mod printer;

use std::cell::Cell;

use uf_infra::Bump;

/// A document: a pointer into the arena.
pub type Doc<'a> = &'a DocNode<'a>;

/// The identity of a group, so an [`if_break`] elsewhere in the tree can ask
/// whether *that* group broke rather than its nearest enclosing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GroupId(pub u32);

/// What a line break is when the enclosing group is flat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineMode {
    /// Nothing when flat, a newline when broken.
    Soft,
    /// A space when flat, a newline when broken.
    Space,
    /// Always a newline. Breaks every enclosing group.
    Hard,
    /// Always a newline, and the next line starts at the root indentation
    /// rather than the current one — for template literal text, which must
    /// not be re-indented.
    Literal,
}

/// What an [`align`] adds to the indentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignKind {
    /// This many extra spaces.
    Spaces(u16),
    /// Back to the indentation marked as root, or to column zero.
    DedentToRoot,
    /// Mark the current indentation as the root [`AlignKind::DedentToRoot`]
    /// returns to.
    Root,
    /// One indentation level less.
    Dedent,
}

/// A tag a printer leaves on a doc so a parent can recognise its shape.
///
/// Prettier's assignment layout asks whether the right-hand side is a member
/// chain; the chain printer answers by labelling its result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    /// The result of the member chain printer.
    MemberChain,
}

/// The shapes a doc can take. See the module docs for what each means to the
/// printer.
#[derive(Debug, Clone, Copy)]
pub enum DocKind<'a> {
    /// Literal text, printed as-is. Never contains a newline; a printer that
    /// needs one uses [`LineMode::Literal`].
    Text(&'a str),
    /// A sequence.
    Concat(&'a [Doc<'a>]),
    /// A possible or forced line break.
    Line(LineMode),
    /// Contents that are printed flat when they fit, and broken otherwise.
    Group {
        /// What to print.
        contents: Doc<'a>,
        /// Alternatives to try in order when the flat form does not fit; the
        /// last one is used broken. Prettier's `conditionalGroup`.
        expanded_states: Option<&'a [Doc<'a>]>,
        /// Forced broken, either explicitly or because the contents hold a
        /// hard break.
        should_break: bool,
        /// Identity for [`DocKind::IfBreak`] and [`DocKind::IndentIfBreak`].
        id: Option<GroupId>,
    },
    /// One level deeper for every line inside.
    Indent(Doc<'a>),
    /// A custom change to the indentation for every line inside.
    Align(AlignKind, Doc<'a>),
    /// One of two docs depending on whether a group broke: the enclosing one,
    /// or the one named by `group_id`.
    IfBreak {
        /// Printed when the group is broken.
        break_contents: Doc<'a>,
        /// Printed when the group is flat.
        flat_contents: Doc<'a>,
        /// Which group to consult; the enclosing one when [`None`].
        group_id: Option<GroupId>,
    },
    /// [`DocKind::Indent`] applied only if the named group broke (or, with
    /// `negate`, only if it did not).
    IndentIfBreak {
        /// What to print.
        contents: Doc<'a>,
        /// The group whose mode decides.
        group_id: GroupId,
        /// Indent when the group is flat instead of when it is broken.
        negate: bool,
    },
    /// Content and separators alternating, packed as many to a line as fit.
    /// Prose, JSX children, and long arrays of numbers use it.
    Fill(&'a [Doc<'a>]),
    /// Deferred until the next newline: trailing comments.
    LineSuffix(Doc<'a>),
    /// Flush pending line suffixes here, with a newline if there are any.
    LineSuffixBoundary,
    /// Break every enclosing group without printing anything.
    BreakParent,
    /// Drop trailing whitespace already printed on the current line.
    Trim,
    /// A [`Label`] on a doc, otherwise transparent.
    Label(Label, Doc<'a>),
}

/// One node of a document. See [`DocKind`].
#[derive(Debug, Clone, Copy)]
pub struct DocNode<'a> {
    /// The node's shape.
    pub kind: DocKind<'a>,
    /// Whether printing this doc forces every enclosing group to break: it
    /// holds a hard line or a [`DocKind::BreakParent`] that is not sealed
    /// inside a conditional group.
    pub breaks: bool,
}

const fn leaf(kind: DocKind<'static>) -> DocNode<'static> {
    DocNode {
        kind,
        breaks: false,
    }
}

/// Nothing.
pub static EMPTY: DocNode<'static> = leaf(DocKind::Text(""));
/// A single space.
pub static SPACE: DocNode<'static> = leaf(DocKind::Text(" "));
/// A break that prints as a space when flat.
pub static LINE: DocNode<'static> = leaf(DocKind::Line(LineMode::Space));
/// A break that prints as nothing when flat.
pub static SOFTLINE: DocNode<'static> = leaf(DocKind::Line(LineMode::Soft));
/// A forced break, which also breaks every enclosing group.
pub static HARDLINE: DocNode<'static> = DocNode {
    kind: DocKind::Concat(&[&HARDLINE_ONLY, &BREAK_PARENT]),
    breaks: true,
};
/// A forced break that does not break enclosing groups by itself.
///
/// Used by the printer to flush line suffixes; node printers want
/// [`HARDLINE`].
pub static HARDLINE_ONLY: DocNode<'static> = DocNode {
    kind: DocKind::Line(LineMode::Hard),
    breaks: true,
};
/// A forced break back to the root indentation.
pub static LITERALLINE: DocNode<'static> = DocNode {
    kind: DocKind::Concat(&[&LITERALLINE_ONLY, &BREAK_PARENT]),
    breaks: true,
};
static LITERALLINE_ONLY: DocNode<'static> = DocNode {
    kind: DocKind::Line(LineMode::Literal),
    breaks: true,
};
/// Break every enclosing group.
pub static BREAK_PARENT: DocNode<'static> = DocNode {
    kind: DocKind::BreakParent,
    breaks: true,
};
/// Flush pending line suffixes.
pub static LINE_SUFFIX_BOUNDARY: DocNode<'static> = leaf(DocKind::LineSuffixBoundary);
/// Drop trailing whitespace on the current line.
pub static TRIM: DocNode<'static> = leaf(DocKind::Trim);

/// Where docs are allocated, and where group ids come from.
pub struct Docs<'a> {
    arena: &'a Bump,
    next_group: Cell<u32>,
}

impl<'a> Docs<'a> {
    /// A builder allocating into `arena`.
    pub fn new(arena: &'a Bump) -> Self {
        Self {
            arena,
            next_group: Cell::new(0),
        }
    }

    /// A fresh group identity.
    pub fn group_id(&self) -> GroupId {
        let id = self.next_group.get();
        self.next_group.set(id + 1);
        GroupId(id)
    }

    /// How many group ids have been handed out, which sizes the printer's
    /// mode table.
    pub fn group_count(&self) -> usize {
        self.next_group.get() as usize
    }

    fn node(&self, kind: DocKind<'a>, breaks: bool) -> Doc<'a> {
        self.arena.alloc(DocNode { kind, breaks })
    }

    /// Text copied into the arena.
    pub fn text(&self, text: &str) -> Doc<'a> {
        if text.is_empty() {
            return &EMPTY;
        }
        let text: &'a str = self.arena.alloc_str(text);
        self.node(DocKind::Text(text), false)
    }

    /// Text that already lives as long as the arena — a slice of the source,
    /// or a literal.
    pub fn borrowed(&self, text: &'a str) -> Doc<'a> {
        if text.is_empty() {
            return &EMPTY;
        }
        self.node(DocKind::Text(text), false)
    }

    /// A slice of docs in the arena.
    pub fn slice<I>(&self, parts: I) -> &'a [Doc<'a>]
    where
        I: IntoIterator<Item = Doc<'a>>,
        I::IntoIter: ExactSizeIterator,
    {
        self.arena.alloc_slice_fill_iter(parts)
    }

    /// The parts printed one after another.
    pub fn concat<I>(&self, parts: I) -> Doc<'a>
    where
        I: IntoIterator<Item = Doc<'a>>,
    {
        let parts: Vec<Doc<'a>> = parts.into_iter().collect();
        self.concat_vec(parts)
    }

    /// [`Docs::concat`] over an already collected vector.
    pub fn concat_vec(&self, parts: Vec<Doc<'a>>) -> Doc<'a> {
        match parts.len() {
            0 => &EMPTY,
            1 => parts[0],
            _ => {
                let breaks = parts.iter().any(|part| part.breaks);
                let parts = self.arena.alloc_slice_copy(&parts);
                self.node(DocKind::Concat(parts), breaks)
            }
        }
    }

    /// Two docs in sequence.
    pub fn pair(&self, first: Doc<'a>, second: Doc<'a>) -> Doc<'a> {
        self.concat_vec(vec![first, second])
    }

    /// A group: flat if it fits, broken otherwise.
    pub fn group(&self, contents: Doc<'a>) -> Doc<'a> {
        self.group_with(contents, false, None)
    }

    /// A group that may be forced broken and may carry an id.
    pub fn group_with(
        &self,
        contents: Doc<'a>,
        should_break: bool,
        id: Option<GroupId>,
    ) -> Doc<'a> {
        let should_break = should_break || contents.breaks;
        self.node(
            DocKind::Group {
                contents,
                expanded_states: None,
                should_break,
                id,
            },
            should_break,
        )
    }

    /// A group that tries each state in turn, using the last one broken.
    ///
    /// Hard breaks inside the states do not leak out to enclosing groups —
    /// the point of a conditional group is that a state may hold them and
    /// still not be chosen.
    pub fn conditional_group(&self, states: &[Doc<'a>], should_break: bool) -> Doc<'a> {
        let states = self.arena.alloc_slice_copy(states);
        let contents = states.first().copied().unwrap_or(&EMPTY);
        self.node(
            DocKind::Group {
                contents,
                expanded_states: Some(states),
                should_break,
                id: None,
            },
            should_break,
        )
    }

    /// One indentation level deeper.
    pub fn indent(&self, contents: Doc<'a>) -> Doc<'a> {
        self.node(DocKind::Indent(contents), contents.breaks)
    }

    /// `n` extra spaces of indentation.
    pub fn align(&self, n: u16, contents: Doc<'a>) -> Doc<'a> {
        self.node(
            DocKind::Align(AlignKind::Spaces(n), contents),
            contents.breaks,
        )
    }

    /// Back to the root indentation for every line inside.
    pub fn dedent_to_root(&self, contents: Doc<'a>) -> Doc<'a> {
        self.node(
            DocKind::Align(AlignKind::DedentToRoot, contents),
            contents.breaks,
        )
    }

    /// Mark the current indentation as root for the lines inside.
    pub fn mark_as_root(&self, contents: Doc<'a>) -> Doc<'a> {
        self.node(DocKind::Align(AlignKind::Root, contents), contents.breaks)
    }

    /// One indentation level shallower.
    pub fn dedent(&self, contents: Doc<'a>) -> Doc<'a> {
        self.node(DocKind::Align(AlignKind::Dedent, contents), contents.breaks)
    }

    /// `break_contents` if the enclosing (or named) group broke, else
    /// `flat_contents`.
    pub fn if_break(
        &self,
        break_contents: Doc<'a>,
        flat_contents: Doc<'a>,
        group_id: Option<GroupId>,
    ) -> Doc<'a> {
        self.node(
            DocKind::IfBreak {
                break_contents,
                flat_contents,
                group_id,
            },
            break_contents.breaks || flat_contents.breaks,
        )
    }

    /// Indent `contents` only if the named group broke.
    pub fn indent_if_break(&self, contents: Doc<'a>, group_id: GroupId, negate: bool) -> Doc<'a> {
        self.node(
            DocKind::IndentIfBreak {
                contents,
                group_id,
                negate,
            },
            contents.breaks,
        )
    }

    /// Alternating content and separators, packed to fit.
    pub fn fill(&self, parts: Vec<Doc<'a>>) -> Doc<'a> {
        let breaks = parts.iter().any(|part| part.breaks);
        let parts = self.arena.alloc_slice_copy(&parts);
        self.node(DocKind::Fill(parts), breaks)
    }

    /// Defer `contents` to the end of the line.
    pub fn line_suffix(&self, contents: Doc<'a>) -> Doc<'a> {
        self.node(DocKind::LineSuffix(contents), contents.breaks)
    }

    /// Tag a doc.
    pub fn label(&self, label: Label, contents: Doc<'a>) -> Doc<'a> {
        self.node(DocKind::Label(label, contents), contents.breaks)
    }

    /// `parts` separated by `separator`.
    pub fn join(&self, separator: Doc<'a>, parts: impl IntoIterator<Item = Doc<'a>>) -> Doc<'a> {
        let mut out = Vec::new();
        for (index, part) in parts.into_iter().enumerate() {
            if index > 0 {
                out.push(separator);
            }
            out.push(part);
        }
        self.concat_vec(out)
    }
}

/// Whether printing `doc` will certainly produce a line break: Prettier's
/// `willBreak`.
///
/// Unlike [`DocNode::breaks`], this looks into a conditional group's first
/// state, because the question is "would the flat attempt break" rather than
/// "does this doc break its parents".
pub fn will_break(doc: Doc<'_>) -> bool {
    if doc.breaks {
        return true;
    }
    match doc.kind {
        DocKind::Group {
            contents,
            should_break,
            ..
        } => should_break || will_break(contents),
        DocKind::Concat(parts) | DocKind::Fill(parts) => parts.iter().any(|part| will_break(part)),
        DocKind::Indent(contents)
        | DocKind::Align(_, contents)
        | DocKind::LineSuffix(contents)
        | DocKind::Label(_, contents)
        | DocKind::IndentIfBreak { contents, .. } => will_break(contents),
        DocKind::IfBreak {
            break_contents,
            flat_contents,
            ..
        } => will_break(break_contents) || will_break(flat_contents),
        DocKind::Line(LineMode::Hard | LineMode::Literal) | DocKind::BreakParent => true,
        DocKind::Text(_) | DocKind::Line(_) | DocKind::LineSuffixBoundary | DocKind::Trim => false,
    }
}

/// Whether `doc` holds any line break at all, hard or soft: Prettier's
/// `canBreak`.
pub fn can_break(doc: Doc<'_>) -> bool {
    match doc.kind {
        DocKind::Line(_) => true,
        DocKind::Text(_) | DocKind::LineSuffixBoundary | DocKind::BreakParent | DocKind::Trim => {
            false
        }
        DocKind::Concat(parts) | DocKind::Fill(parts) => parts.iter().any(|part| can_break(part)),
        DocKind::Group { contents, .. }
        | DocKind::Indent(contents)
        | DocKind::Align(_, contents)
        | DocKind::LineSuffix(contents)
        | DocKind::Label(_, contents)
        | DocKind::IndentIfBreak { contents, .. } => can_break(contents),
        DocKind::IfBreak {
            break_contents,
            flat_contents,
            ..
        } => can_break(break_contents) || can_break(flat_contents),
    }
}

/// The text of `doc` when it is a single piece of text, looking through
/// labels.
pub fn text_of<'a>(doc: Doc<'a>) -> Option<&'a str> {
    match doc.kind {
        DocKind::Text(text) => Some(text),
        DocKind::Label(_, contents) => text_of(contents),
        DocKind::Concat(parts) if parts.len() == 1 => text_of(parts[0]),
        _ => None,
    }
}

impl<'a> Docs<'a> {
    /// `doc` with every soft break flattened: lines become spaces or
    /// nothing, `if_break`s take their flat side, groups stay but no longer
    /// break. Hard lines survive. Prettier's `removeLines`.
    pub fn remove_lines(&self, doc: Doc<'a>) -> Doc<'a> {
        match doc.kind {
            DocKind::Line(LineMode::Soft) => &EMPTY,
            DocKind::Line(LineMode::Space) => &SPACE,
            DocKind::Line(_)
            | DocKind::Text(_)
            | DocKind::LineSuffixBoundary
            | DocKind::BreakParent
            | DocKind::Trim => doc,
            DocKind::Concat(parts) => {
                let parts: Vec<Doc<'a>> =
                    parts.iter().map(|part| self.remove_lines(part)).collect();
                self.concat_vec(parts)
            }
            DocKind::Fill(parts) => {
                let parts: Vec<Doc<'a>> =
                    parts.iter().map(|part| self.remove_lines(part)).collect();
                self.fill(parts)
            }
            DocKind::Group {
                contents,
                expanded_states,
                should_break,
                id,
            } => {
                let contents = self.remove_lines(contents);
                match expanded_states {
                    Some(states) => {
                        let states: Vec<Doc<'a>> = states
                            .iter()
                            .map(|state| self.remove_lines(state))
                            .collect();
                        self.conditional_group(&states, should_break)
                    }
                    None => self.group_with(contents, should_break, id),
                }
            }
            DocKind::Indent(contents) => self.indent(self.remove_lines(contents)),
            DocKind::Align(kind, contents) => {
                let contents = self.remove_lines(contents);
                self.node(DocKind::Align(kind, contents), contents.breaks)
            }
            DocKind::IfBreak { flat_contents, .. } => self.remove_lines(flat_contents),
            DocKind::IndentIfBreak {
                contents,
                group_id,
                negate,
            } => self.indent_if_break(self.remove_lines(contents), group_id, negate),
            DocKind::LineSuffix(contents) => self.line_suffix(self.remove_lines(contents)),
            DocKind::Label(label, contents) => self.label(label, self.remove_lines(contents)),
        }
    }
}

/// Whether `doc` prints nothing at all.
pub fn is_empty(doc: Doc<'_>) -> bool {
    match doc.kind {
        DocKind::Text(text) => text.is_empty(),
        DocKind::Concat(parts) => parts.iter().all(|part| is_empty(part)),
        _ => false,
    }
}

/// The label on `doc`, looking through nothing else.
pub fn label_of(doc: Doc<'_>) -> Option<Label> {
    match doc.kind {
        DocKind::Label(label, _) => Some(label),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breaks_propagate_to_groups_but_not_through_conditional_groups() {
        let arena = Bump::new();
        let docs = Docs::new(&arena);
        let inner = docs.group(docs.concat([docs.text("a"), &HARDLINE, docs.text("b")]));
        assert!(inner.breaks);
        let outer = docs.group(docs.concat([docs.text("("), inner, docs.text(")")]));
        assert!(matches!(
            outer.kind,
            DocKind::Group {
                should_break: true,
                ..
            }
        ));

        let sealed = docs.conditional_group(&[inner, docs.text("flat")], false);
        assert!(!sealed.breaks);
        assert!(will_break(sealed));
        let parent = docs.group(docs.concat([docs.text("("), sealed]));
        assert!(!parent.breaks);
    }

    #[test]
    fn concat_flattens_trivial_cases() {
        let arena = Bump::new();
        let docs = Docs::new(&arena);
        assert!(is_empty(docs.concat([])));
        let one = docs.text("x");
        assert!(std::ptr::eq(docs.concat([one]), one));
        assert!(is_empty(docs.text("")));
    }
}
