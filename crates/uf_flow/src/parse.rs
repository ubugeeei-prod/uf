//! The parse entry point: the official port's syntax tree for one source file.
//!
//! [`validate_source`](crate::validate_source) answers "does this parse", which
//! is what the linter asks. Anything that *rewrites* source — the formatter, a
//! transform — needs the tree itself, and it needs the parser's own tree rather
//! than a copy: a second definition of Flow syntax is a second place for the
//! grammar to drift, which is the mistake this crate exists to prevent. So the
//! port's types are re-exported here as [`ast`] and [`Loc`], and [`parse`]
//! hands back a [`Parsed`] built from them.
//!
//! [`ast_visitor`] comes with them, for the same reason. A caller that only
//! wants one kind of node still has to descend through every other kind to
//! find it, and a hand-written descent is a second definition of Flow syntax:
//! it goes quietly out of date on the next `tools/upstream/sync.sh`, with a
//! rule that stopped firing as the symptom. The port's own visitor walks
//! whatever the port parses.
//!
//! # Ceilings
//!
//! The port is a recursive-descent parser, and a stack overflow cannot be
//! caught, so two limits are enforced *before* it runs: [`MAX_PARSE_BYTES`] on
//! the size of the source and [`MAX_NESTING_DEPTH`] on how deeply brackets
//! nest. Both come back as a typed [`ParseFailure`] so a caller formatting a
//! whole project sees one refused file rather than its own crash. Syntax errors
//! are deliberately not failures: the port recovers and reports them, and they
//! ride along as [`Parsed::diagnostics`].
//!
//! # Freeing the tree
//!
//! The ceilings bound what the *parser* recurses through. Freeing the tree
//! recurses too, once per level, and it happens wherever the [`Parsed`] is
//! held rather than where it was built — so a caller who parsed on a large
//! stack and then carried the result back to an ordinary one aborted the
//! process on a source every ceiling here accepts. [`Parsed`]'s [`Drop`]
//! owns that now; see the type for why it is the type's job and not the
//! caller's.

use std::panic::{AssertUnwindSafe, catch_unwind};

use flow_parser::ParseOptions;
use thiserror::Error;

pub use flow_parser::ast;
pub use flow_parser::ast_visitor;
pub use flow_parser::loc::{Loc, Position};

use crate::ParseDiagnostic;

/// Longest source [`parse`] will accept, in bytes.
///
/// The same ceiling as [`MAX_STRIP_BYTES`](crate::MAX_STRIP_BYTES), for the
/// same reason: a dependency can put a generated or hostile file in
/// `node_modules`, and every scan in `uf` has an explicit limit above it rather
/// than trusting the input to be a reasonable size.
pub const MAX_PARSE_BYTES: usize = 8 * 1024 * 1024;

/// Deepest bracket nesting [`parse`] will accept.
///
/// The port spends stack in proportion to this number, and a formatter that
/// walks the tree recursively again doubles the exposure. A file that nests
/// brackets hundreds deep is not something a person wrote; refusing it with a
/// typed error is the alternative to a stack overflow. The number is generous
/// for real code — generated tables rarely pass fifty — and, together with
/// [`PARSE_STACK_BYTES`], one the parser has been measured to survive.
pub const MAX_NESTING_DEPTH: usize = 300;

/// Longest chain of operators [`parse`] will accept at one bracket level.
///
/// `1 + 1 + 1 + …` and `a.f().f()…` nest one AST node per link and open no
/// bracket that stays open, so [`MAX_NESTING_DEPTH`] does not see them at
/// all: a 3 MB file of the first measured **0** and aborted `uf fmt` with a
/// stack overflow. See ubugeeei-prod/uf#136.
///
/// A separate number because a level of brackets and a level of `+` cost
/// very different amounts of stack — the port's object-literal frame is
/// about 150 KiB and a binary operand is a small fraction of that, which is
/// why this ceiling is thirty times the other one and still lower than
/// where the printer gives out.
///
/// Measured from both sides.
///
/// **What real code reaches.** The 15,971 files in `tests/fixtures/git` —
/// React, React Native, Metro, Relay, Parcel, Yarn, Prepack and eight more,
/// with minified third-party bundles among them — reach **1,958**, in
/// CodeMirror's bundle. So this is five times the deepest chain anyone in the
/// corpus wrote.
///
/// **What the formatter survives.** `uf fmt` runs the parser, the printer and
/// the tree's `Drop` on a thread of [`PARSE_STACK_BYTES`], and formats a
/// chain of 40,000 there; it gives out somewhere before 100,000. So this is
/// four times under the measured floor of the path that ships.
///
/// # A caller that held the tree on a small stack
///
/// Freeing the tree recurses once per level too, and it used to happen on
/// whatever thread held the [`Parsed`]. A 2 MiB thread — an unoptimized test
/// thread is one — frees about 6,900 levels, which is *below* this ceiling,
/// so a caller that parsed where there was room and carried the result back
/// aborted. Lowering this number was not the answer: the corpus above
/// already reaches 1,958, so there is no room between what real code writes
/// and what a 2 MiB stack frees. [`Parsed`]'s [`Drop`] is the answer, and
/// this ceiling is still the one the parser and the printer were measured
/// against. See ubugeeei-prod/uf#155.
pub const MAX_CHAIN_DEPTH: usize = 10_000;

/// Stack a thread needs to run [`parse`] on any source under
/// [`MAX_NESTING_DEPTH`].
///
/// The port's frames are large: measured on an unoptimized build, an object
/// literal costs about 150 KiB of stack per level of nesting, so the ceiling
/// alone needs some 45 MiB — far more than the 8 MiB a main thread or the
/// 2 MiB a test thread gets. Callers run the parser (and whatever recursive
/// walk they do over its tree) on a thread of this size; the reservation is
/// virtual and only the pages actually touched cost anything. A test below
/// parses at the ceiling on such a thread, so the two constants cannot drift
/// apart unnoticed.
pub const PARSE_STACK_BYTES: usize = 128 * 1024 * 1024;

/// Deepest tree [`Parsed`] frees on the thread that holds it.
///
/// Above this it goes to a thread of [`PARSE_STACK_BYTES`] instead, which is
/// what keeps a caller on an ordinary stack from aborting on a tree it is
/// merely letting go of. The number decides only where the free runs; it
/// refuses nothing and it is not a ceiling.
///
/// **Measured.** Freeing costs about 300 bytes of stack per level in an
/// unoptimized build — a 2 MiB thread, which is what a thread gets when
/// nobody chooses otherwise, gave out between 6,906 and 6,925 levels across
/// every shape tried (`a.f().f()…`, `1 + 1 + …`, `f()()…`, `a.b.b…`,
/// `typeof typeof …`, `await await …`), and about 210 bytes per level
/// optimized. So this is under a sixth of the measured floor of the slower
/// build, roughly 300 KiB of the 2 MiB, and it leaves the rest of that stack
/// to the caller — which matters, because a caller is never at the bottom of
/// its own thread when it drops something.
///
/// **And above what anyone writes.** The 15,971 files of
/// `tests/fixtures/git` reach 1,958, in CodeMirror's minified bundle. Of the
/// 187 Flow sources this repository ships the deepest is 154, and of 2,504
/// third-party modules under `node_modules` — React, Babel, Rolldown, Shiki
/// and the rest, development builds included — the deepest is 759. None of
/// the 2,691 is over this number, so on real input the comparison in
/// [`Parsed::drop`] is all that ever runs and no thread is started at all.
///
/// **What it costs when it is crossed.** Freeing a tree of 1,025 levels took
/// 111 µs against 67 µs in place, and one at [`MAX_CHAIN_DEPTH`] took
/// 1.2 ms: a thread with this much stack reserved costs some tens of
/// microseconds to start and join, once, against a file that takes
/// milliseconds to parse. `uf_fmt` and `uf_doc` pay it on such a file even
/// though their own threads had the room — the [`Parsed`] cannot know what
/// stack it is standing on, and a way to tell it would be a way to tell it
/// wrong.
pub const MAX_DEPTH_FREED_IN_PLACE: usize = 1_024;

/// Parse options aligned with the `uf` project defaults.
///
/// Every syntax `uf` ships in templates or lints is on — component and hook
/// syntax, enums, pattern matching, records, Flow types, and types in
/// comments. Decorators stay off because generated projects never emit them.
///
/// # The only copy
///
/// Everything in uf that hands source to the Flow parser hands it *this*:
/// [`parse`], [`validate_source`](crate::validate_source), the transform in
/// `uf_transform`, and `uf_check`'s checker. There were three literals, equal
/// member for member by coincidence, each with a comment claiming it mirrored
/// one of the others. Nothing checked that, and a file that parses for the
/// linter and not for the formatter is worse than one that parses for neither:
/// the linter says the file is fine, the formatter refuses it, and the reader
/// has no way to tell which is right.
///
/// Then the checker became a fourth entry point and passed the port's
/// `PERMISSIVE_PARSE_OPTIONS` instead — `esproposal_decorators` on where every
/// other command has it off, so a decorated class checked and would not format
/// (ubugeeei-prod/uf#430). [`module::parse`](crate::module::parse) takes no
/// options argument now, so there is nothing left for an entry point to
/// disagree with; the constant is read there, once.
///
/// `uf_transform`'s `tests/parse_options.rs` is what keeps it one copy. It
/// runs the same syntax through all four entry points and requires the same
/// answer; it reads the workspace to find any *fifth* place that reaches the
/// port's parser; and it requires every member of [`ParseOptions`] to decide
/// at least one of those samples — so a member added upstream fails the test
/// until a sample covers it, rather than drifting unwatched.
///
/// # Why decorators stay off, when Flow leaves them on
///
/// `esproposal_decorators` is the one member where uf's answer differs from
/// the checker it embeds, and it is a decision rather than an oversight.
/// `flow check` at 0.330.0 accepts `@decorate class Thing {}` and reports
/// nothing — not even `cannot-resolve-name` for a decorator that is not
/// declared, because it parses the syntax and then types none of it. Flow can
/// afford that: it only ever *reads* a file.
///
/// uf also has to emit one. `uf fmt` prints the tree back, [`crate::strip`]
/// erases types to get the JavaScript a browser runs, and `uf transform`
/// lowers it — and none of the three has a rule for a decorator. Accepting the
/// syntax in the checker alone would mean `uf check` blessing a module that
/// `uf fmt`, `uf test` and `uf build` cannot process, which is a worse answer
/// than one syntax error from all four.
pub const PARSE_OPTIONS: ParseOptions = ParseOptions {
    components: true,
    enums: true,
    pattern_matching: true,
    records: true,
    esproposal_decorators: false,
    types: true,
    ambiguous_types: true,
    enable_types_in_comments: true,
    use_strict: false,
    assert_operator: false,
    module_ref_prefix: None,
    ambient: false,
    allow_return_outside_function: false,
};

/// One Flow source file, parsed by the official port.
///
/// The port recovers from syntax errors, so a program is always produced;
/// [`Parsed::diagnostics`] says whether it is the program the author meant.
/// Anything that rewrites source from the tree must refuse to when there are
/// diagnostics, because a recovered tree is the parser's best guess and
/// printing a guess loses code.
///
/// # Freeing it is this type's job, not the caller's
///
/// The port's types free themselves recursively, so letting a tree go costs
/// stack in proportion to its depth — on whatever thread holds it, which is
/// not the one that built it. [`parse`] wants [`PARSE_STACK_BYTES`] and says
/// so; a caller who obliges, takes the [`Parsed`] home and drops it on an
/// ordinary 2 MiB thread aborts there, on a source under every ceiling this
/// module declares, with nothing to catch. That is ubugeeei-prod/uf#155.
///
/// Documenting the requirement instead — "hold a `Parsed` only on a big
/// stack" — was the other candidate, and it was rejected for what it does
/// not cover. `uf_fmt`, `uf_doc` and `uf_check` already run on their own
/// large stacks, so a rule would have cost nothing today and been enforced
/// by a test per crate; but the rule only binds the callers someone
/// remembers to write a test for, `parse` is public and its next caller is
/// not in this repository, and the obligation is invisible at the point it
/// is broken — a `Parsed` moved into a channel, a `Vec`, or a `catch_unwind`
/// boundary changes threads without anybody writing down that it did. A
/// caller can decline to walk a tree; nobody can decline to free one, so the
/// unavoidable half belongs to the type.
///
/// Reading it is still the caller's own recursion on the caller's own stack,
/// and [`parse`] still documents the stack that needs. That half is a
/// choice, and a choice can be documented.
#[derive(Debug, Clone)]
pub struct Parsed {
    /// The syntax tree, in the port's own types.
    pub program: ast::Program<Loc, Loc>,
    /// Syntax errors in source order; empty for a clean parse.
    pub diagnostics: Vec<ParseDiagnostic>,
    /// How deeply the source nested, by whichever of [`Depths`]' two
    /// measures is larger.
    ///
    /// Kept rather than recomputed: [`parse`] has already scanned the source
    /// to decide the ceilings, so this costs nothing, and by the time
    /// [`Parsed::drop`] runs the source it came from is long gone.
    depth: usize,
}

impl Drop for Parsed {
    /// Free a deep tree on a thread with room for it.
    ///
    /// # The three things it must not do
    ///
    /// **Leak a thread per parse.** The worker is joined here, so it is gone
    /// before `drop` returns and the memory is really back by then —
    /// `drop(parsed)` means what it says, and a caller freeing trees in a
    /// loop accumulates neither threads nor bytes. Spawning and *not*
    /// joining was the first shape written and it is wrong twice over: a
    /// thread per parse, and a `Parsed` that has not actually been freed
    /// when the caller's next line runs.
    ///
    /// **Cost the ordinary file anything.** Everything at or under
    /// [`MAX_DEPTH_FREED_IN_PLACE`] — which is everything a person writes —
    /// takes the early return below: one comparison against a number
    /// [`parse`] had already computed, and no thread at all.
    ///
    /// **Give up during a panic.** `Drop` runs while unwinding too, so the
    /// worker's result is deliberately dropped rather than unwrapped: a
    /// panic raised here would abort a process that is already handling one.
    fn drop(&mut self) {
        if self.depth <= MAX_DEPTH_FREED_IN_PLACE {
            return;
        }

        // Only the statements nest. `loc`, the interpreter directive and the
        // two comment lists are flat and cost nothing to free here, and
        // taking the one recursive field means this does not have to name
        // every member the port's `Program` has — one added upstream would
        // otherwise have to be added here too, and `tools/upstream/sync.sh`
        // is not somewhere that gets noticed. What is left behind is the
        // empty slice, so the `Parsed` stays a whole value for the rest of
        // its own drop.
        let statements = std::mem::take(&mut self.program.statements);

        // Walking the tree with an explicit worklist and freeing it
        // bottom-up needs no thread at all, and was the first answer tried.
        // It is the wrong one: an iterative free has to know the shape of
        // every node the port declares, which is a second definition of Flow
        // syntax living in `uf` — the drift this crate exists to prevent —
        // and it would go quietly out of date on the next upstream sync,
        // with a stack overflow as the symptom.
        let Ok(worker) = std::thread::Builder::new()
            .name("uf-flow-free".into())
            .stack_size(PARSE_STACK_BYTES)
            .spawn(move || drop(statements))
        else {
            // The thread would not start, which means the process is out of
            // memory or out of thread handles. `statements` went down with
            // the closure, on this stack, which is the overflow this exists
            // to prevent — there is nowhere else to put it, and a process
            // that cannot start a thread is failing regardless.
            return;
        };
        let _ = worker.join();
    }
}

impl Parsed {
    /// Whether the source parsed without a single syntax error.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Every comment in the file, in source order, whether or not the parser
    /// attached it to a node.
    ///
    /// Comment text excludes its delimiters: `// a` carries `" a"` and
    /// `/* b */` carries `" b "`. A printer adds them back from
    /// [`ast::Comment::kind`].
    #[must_use]
    pub fn comments(&self) -> &[ast::Comment<Loc>] {
        &self.program.all_comments
    }

    /// Whether [`Parsed::drop`] will start a thread to free this tree.
    ///
    /// Test-only, because which side of [`MAX_DEPTH_FREED_IN_PLACE`] a tree
    /// falls on is not something a caller can act on: the answer is either
    /// "freed" or "freed", and only the stack it happens on differs. It
    /// exists so a test can assert that ordinary source takes the free path,
    /// which is the half of the fix that an overflow cannot demonstrate.
    #[cfg(test)]
    fn freed_off_thread(&self) -> bool {
        self.depth > MAX_DEPTH_FREED_IN_PLACE
    }
}

/// Why [`parse`] refused a source before the parser saw it.
///
/// Syntax errors are not a failure: they come back as
/// [`Parsed::diagnostics`], with the recovered tree beside them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ParseFailure {
    /// The source is larger than [`MAX_PARSE_BYTES`].
    #[error("source is {bytes} bytes, over the {limit} byte ceiling")]
    SourceTooLarge {
        /// Size of the rejected source.
        bytes: usize,
        /// The ceiling, always [`MAX_PARSE_BYTES`].
        limit: usize,
    },
    /// Brackets nest deeper than [`MAX_NESTING_DEPTH`].
    #[error("brackets nest {depth} deep, over the {limit} level ceiling")]
    TooDeeplyNested {
        /// The nesting the scanner measured.
        depth: usize,
        /// The ceiling, always [`MAX_NESTING_DEPTH`].
        limit: usize,
    },
    /// Operators chain deeper than [`MAX_CHAIN_DEPTH`].
    ///
    /// Separate from [`TooDeeplyNested`](Self::TooDeeplyNested) because the
    /// two are different shapes and a message that says "brackets" about
    /// `1 + 1 + 1 + …` sends whoever reads it looking for brackets.
    #[error("operators chain {depth} deep, over the {limit} level ceiling")]
    TooDeeplyChained {
        /// The chain the scanner measured.
        depth: usize,
        /// The ceiling, always [`MAX_CHAIN_DEPTH`].
        limit: usize,
    },
    /// The port panicked instead of reporting a diagnostic.
    ///
    /// Not expected for any input, but a parser is the part of a toolchain
    /// most exposed to hostile bytes, and a caller that formats files in bulk
    /// must see one bad file as an error, not as its own crash.
    #[error("the Flow parser failed on this source")]
    ParserPanicked,
}

/// Parse `source` with the official Flow port and return its syntax tree.
///
/// # Call this from a thread with [`PARSE_STACK_BYTES`] of stack
///
/// The port is recursive descent and its frames are large, so parsing a
/// source at [`MAX_NESTING_DEPTH`] needs the room; this runs on the caller's
/// thread and does not find that room for itself. So does any recursive walk
/// the caller then does over the tree, which is the more easily forgotten
/// half — a main thread's 8 MiB is not enough to read a source at
/// [`MAX_CHAIN_DEPTH`], which is inside every limit here.
///
/// *Freeing* the tree is the exception, and it needs nothing from the
/// caller: [`Parsed`] takes a deep one to a thread of this size itself. See
/// that type for why the free is the only half that could be taken away from
/// the caller, and ubugeeei-prod/uf#155 for what it cost when it was not.
///
/// `uf_fmt` and `uf_doc` both give the parser this stack.
///
/// # Errors
///
/// Returns [`ParseFailure::SourceTooLarge`] past [`MAX_PARSE_BYTES`],
/// [`ParseFailure::TooDeeplyNested`] past [`MAX_NESTING_DEPTH`] and
/// [`ParseFailure::TooDeeplyChained`] past [`MAX_CHAIN_DEPTH`]; all three are
/// decided before the parser runs, which is the point — the tree that would
/// overflow the stack is never built. Syntax errors are not errors here: see
/// [`Parsed::diagnostics`].
pub fn parse(source: &str) -> Result<Parsed, ParseFailure> {
    if source.len() > MAX_PARSE_BYTES {
        return Err(ParseFailure::SourceTooLarge {
            bytes: source.len(),
            limit: MAX_PARSE_BYTES,
        });
    }
    let depths = depths(source);
    if depths.brackets > MAX_NESTING_DEPTH {
        return Err(ParseFailure::TooDeeplyNested {
            depth: depths.brackets,
            limit: MAX_NESTING_DEPTH,
        });
    }
    if depths.chain > MAX_CHAIN_DEPTH {
        return Err(ParseFailure::TooDeeplyChained {
            depth: depths.chain,
            limit: MAX_CHAIN_DEPTH,
        });
    }

    let (program, errors) = catch_unwind(AssertUnwindSafe(|| crate::module::parse(source, None)))
        .map_err(|_| ParseFailure::ParserPanicked)?;

    Ok(Parsed {
        program,
        diagnostics: errors
            .iter()
            .map(|error| crate::diagnostic_from_error(source, error))
            .collect(),
        // The larger of the two measures, because either one of them is a
        // level the tree nests and so a frame the free recurses through.
        depth: depths.brackets.max(depths.chain),
    })
}

/// What the depth scanner is inside: JavaScript, with the number of `{`
/// opened since the frame began, or the literal text of a template.
#[derive(Clone, Copy)]
enum DepthFrame {
    Js { braces: usize },
    TemplateText,
}

/// What the previous significant token was, as far as deciding whether a `/`
/// starts a regular expression needs to know.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Previous {
    /// Nothing yet, or something a division cannot follow.
    Operator,
    /// A value: an identifier, a literal, or a closing bracket.
    Value,
}

/// How deeply `source` nests, in brackets and in operators.
///
/// This is the guard behind [`MAX_NESTING_DEPTH`], so it has to see what the
/// parser sees: brackets inside strings, comments and regular expressions are
/// text, and the nesting hidden inside a template's `${ … }` counts to any
/// depth or the ceiling could be walked around. It is a single pass of its own
/// rather than a walk over [`scan::tokenize`](crate::scan::tokenize), which
/// hands back a whole template literal as one token.
///
/// # Operators nest too
///
/// `(`, `[` and `{` are not the only things that add a level.
/// `1 + 1 + 1 + …` is one `Binary` node per operand and `a.f().f()…` is one
/// `Member` and one `Call` per link, and neither opens a bracket that stays
/// open. Counting brackets alone reported **0** for a 3 MB file of
/// `1 + 1 + …` that aborted `uf fmt` with a stack overflow — inside every
/// limit this module declares. See ubugeeei-prod/uf#136.
///
/// So each bracket level also carries a *run*: how many operator tokens have
/// been seen since the last `,` or `;` at that level. The reset is what keeps
/// the measure from confusing width with depth — `[a + b, c + d, …]` is a
/// thousand siblings of depth one, not a chain of a thousand — and the
/// per-level stack is what keeps an inner expression from being charged for
/// the one it sits in.
///
/// A run of operator bytes counts once, so `===` is one level and not three.
/// The answer is an upper bound: `a + -b` counts two where the tree nests
/// two, and an object literal's `:` counts one where nothing nests. Erring
/// high is the safe direction for a ceiling.
///
/// Unbalanced closers are ignored rather than reported: the answer is an upper
/// bound on how deep the parser will recurse, and the parser is the one that
/// diagnoses the mismatch.
pub fn nesting_depth(source: &str) -> usize {
    depths(source).brackets
}

/// The longest run of operator tokens between two separators, in the deepest
/// bracket level it appears at.
///
/// The guard behind [`MAX_CHAIN_DEPTH`]. See [`nesting_depth`] for why this
/// is a second number rather than part of the first: a level of brackets and
/// a level of `+` cost the parser very different amounts of stack, so one
/// ceiling cannot be right for both.
#[must_use]
pub fn chain_depth(source: &str) -> usize {
    depths(source).chain
}

/// How deeply a source nests, by both measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Depths {
    /// Deepest bracket nesting.
    pub brackets: usize,
    /// Longest operator run, plus the brackets it sits inside.
    pub chain: usize,
}

/// Both measures, in one pass.
#[must_use]
pub fn depths(source: &str) -> Depths {
    let bytes = source.as_bytes();
    let mut at = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };
    if bytes[at..].starts_with(b"#!") {
        at = line_end(bytes, at);
    }

    let mut frames: Vec<DepthFrame> = vec![DepthFrame::Js { braces: 0 }];
    let mut depth = 0usize;
    let mut deepest = 0usize;
    let mut deepest_chain = 0usize;
    let mut previous = Previous::Operator;
    // One `(base, run)` per open bracket, innermost last: how deep this
    // level starts, and how many operator tokens have been seen in it since
    // the last `,` or `;`. The outermost entry is the file's top level,
    // which has no bracket of its own.
    //
    // The base is what makes the answer an upper bound on tree depth rather
    // than on depth-within-a-level: in `a + f(b + c)` the inner `+` sits
    // under a `+`, a call and a paren, and the level it opens says so.
    let mut levels: Vec<(usize, usize)> = vec![(0, 0)];

    while at < bytes.len() {
        let Some(&frame) = frames.last() else {
            frames.push(DepthFrame::Js { braces: 0 });
            continue;
        };

        if let DepthFrame::TemplateText = frame {
            match bytes[at] {
                b'\\' => at += 2,
                b'`' => {
                    frames.pop();
                    previous = Previous::Value;
                    at += 1;
                }
                b'$' if bytes.get(at + 1) == Some(&b'{') => {
                    frames.push(DepthFrame::Js { braces: 0 });
                    depth += 1;
                    levels.push((level_base(&levels) + 1, 0));
                    deepest = deepest.max(depth);
                    previous = Previous::Operator;
                    at += 2;
                }
                _ => at += 1,
            }
            continue;
        }

        let byte = bytes[at];
        match byte {
            b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c => at += 1,
            b'/' if bytes.get(at + 1) == Some(&b'/') => at = line_end(bytes, at),
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                at = match find(bytes, at + 2, b"*/") {
                    Some(end) => end + 2,
                    None => bytes.len(),
                };
            }
            b'"' | b'\'' => {
                at = quoted_end(bytes, at, byte);
                previous = Previous::Value;
            }
            b'`' => {
                frames.push(DepthFrame::TemplateText);
                at += 1;
            }
            b'/' => {
                match previous {
                    Previous::Operator => at = regex_end(bytes, at).unwrap_or(at + 1),
                    // A division, which nests like any other operator. The
                    // regex branch above is why this cannot be left to the
                    // operator arm: `/` is decided here or not at all.
                    Previous::Value => {
                        at += 1;
                        if let Some((base, run)) = levels.last_mut() {
                            *run += 1;
                            deepest_chain = deepest_chain.max(*base + *run);
                        }
                    }
                }
                previous = Previous::Value;
            }
            b'(' | b'[' => {
                // After a value these open a call or an index, and each of
                // those is a node: `f()()()…` and `a[0][0][0]…` nest as
                // deeply as they are long while the bracket depth stays at
                // one. After an operator they are a grouping paren or an
                // array literal, which the bracket count already has.
                if previous == Previous::Value
                    && let Some((base, run)) = levels.last_mut()
                {
                    *run += 1;
                    deepest_chain = deepest_chain.max(*base + *run);
                }
                depth += 1;
                levels.push((level_base(&levels) + 1, 0));
                deepest = deepest.max(depth);
                previous = Previous::Operator;
                at += 1;
            }
            b'{' => {
                if let Some(DepthFrame::Js { braces }) = frames.last_mut() {
                    *braces += 1;
                }
                depth += 1;
                levels.push((level_base(&levels) + 1, 0));
                deepest = deepest.max(depth);
                previous = Previous::Operator;
                at += 1;
            }
            b')' | b']' => {
                depth = depth.saturating_sub(1);
                if levels.len() > 1 {
                    levels.pop();
                }
                previous = Previous::Value;
                at += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                previous = Previous::Value;
                let closes_substitution = match frames.last_mut() {
                    Some(DepthFrame::Js { braces }) if *braces > 0 => {
                        *braces -= 1;
                        false
                    }
                    Some(DepthFrame::Js { .. }) => frames.len() > 1,
                    _ => false,
                };
                if closes_substitution {
                    frames.pop();
                }
                if levels.len() > 1 {
                    levels.pop();
                }
                at += 1;
            }
            b'0'..=b'9' => {
                at = number_end(bytes, at);
                previous = Previous::Value;
            }
            b'.' if matches!(bytes.get(at + 1), Some(b'0'..=b'9')) => {
                at = number_end(bytes, at);
                previous = Previous::Value;
            }
            _ if is_ident_start(byte) => {
                let start = at;
                at += 1;
                while at < bytes.len() && is_ident_part(bytes[at]) {
                    at += 1;
                }
                let word = &bytes[start..at];
                // A prefix operator spelled as a word nests exactly like one
                // spelled in punctuation: `typeof typeof … x` and
                // `new new … Foo` are one node per word, and neither opens a
                // bracket.
                if is_prefix_keyword(word)
                    && let Some((base, run)) = levels.last_mut()
                {
                    *run += 1;
                    deepest_chain = deepest_chain.max(*base + *run);
                }
                previous = if precedes_expression(word) {
                    Previous::Operator
                } else {
                    Previous::Value
                };
            }
            b',' | b';' => {
                if let Some((_, run)) = levels.last_mut() {
                    *run = 0;
                }
                previous = Previous::Operator;
                at += 1;
            }
            _ if is_operator(byte) => {
                // A whole run of operator bytes is one token: `===` nests
                // once, not three times.
                while at < bytes.len() && is_operator(bytes[at]) {
                    at += 1;
                }
                if let Some((base, run)) = levels.last_mut() {
                    *run += 1;
                    deepest_chain = deepest_chain.max(*base + *run);
                }
                previous = Previous::Operator;
            }
            _ => {
                previous = Previous::Operator;
                at += 1;
            }
        }
    }

    Depths {
        brackets: deepest,
        chain: deepest_chain,
    }
}

/// Where a bracket opened inside `levels` starts counting from: everything
/// its enclosing level has already nested.
fn level_base(levels: &[(usize, usize)]) -> usize {
    levels.last().map_or(0, |(base, run)| base + run)
}

/// Whether `word` is a prefix operator: one that takes an expression and is
/// itself an expression, so a run of them nests.
///
/// `typeof`, `void`, `delete`, `await`, `yield` and `new`. Not `return` or
/// `case`, which take an expression and are not one — they cannot repeat.
fn is_prefix_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"typeof" | b"void" | b"delete" | b"await" | b"yield" | b"new"
    )
}

/// Whether `byte` is punctuation that can nest one expression inside
/// another: an operator, a member access, or a ternary's `?` and `:`.
///
/// Brackets, `,` and `;` are deliberately absent — the first are counted as
/// depth already and the second two end a run rather than extend it.
const fn is_operator(byte: u8) -> bool {
    matches!(
        byte,
        b'.' | b'+'
            | b'-'
            | b'*'
            | b'/'
            | b'%'
            | b'<'
            | b'>'
            | b'='
            | b'!'
            | b'&'
            | b'|'
            | b'^'
            | b'~'
            | b'?'
            | b':'
    )
}

/// Keywords after which a `/` begins a regular expression rather than dividing.
fn precedes_expression(word: &[u8]) -> bool {
    matches!(
        word,
        b"await"
            | b"case"
            | b"delete"
            | b"do"
            | b"else"
            | b"in"
            | b"instanceof"
            | b"new"
            | b"of"
            | b"return"
            | b"throw"
            | b"typeof"
            | b"void"
            | b"yield"
    )
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' || byte >= 0x80
}

fn is_ident_part(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit()
}

fn line_end(bytes: &[u8], from: usize) -> usize {
    let mut at = from;
    while at < bytes.len() && !matches!(bytes[at], b'\n' | b'\r') {
        at += 1;
    }
    at
}

fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    bytes
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| from + offset)
}

/// One past the closing quote of the string starting at `from`, or the end
/// of its line for an unterminated one.
fn quoted_end(bytes: &[u8], from: usize, quote: u8) -> usize {
    let mut at = from + 1;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'\n' | b'\r' => return at,
            byte if byte == quote => return at + 1,
            _ => at += 1,
        }
    }
    bytes.len()
}

fn number_end(bytes: &[u8], from: usize) -> usize {
    let mut at = from;
    while at < bytes.len() {
        let byte = bytes[at];
        let exponent_sign = matches!(byte, b'+' | b'-')
            && at > from
            && matches!(bytes[at - 1], b'e' | b'E')
            && !bytes[from..at].starts_with(b"0x")
            && !bytes[from..at].starts_with(b"0X");
        if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.' || exponent_sign {
            at += 1;
        } else {
            break;
        }
    }
    at
}

/// One past the flags of the regular expression starting at `from`, or
/// [`None`] when the slash turns out to be a division after all.
fn regex_end(bytes: &[u8], from: usize) -> Option<usize> {
    let mut at = from + 1;
    let mut in_class = false;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'\n' | b'\r' => return None,
            b'[' => {
                in_class = true;
                at += 1;
            }
            b']' => {
                in_class = false;
                at += 1;
            }
            b'/' if !in_class => {
                at += 1;
                while at < bytes.len() && is_ident_part(bytes[at]) {
                    at += 1;
                }
                return Some(at);
            }
            _ => at += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_the_tree_and_every_comment() {
        let source = "// @flow\n/* a */ const x = 1; // b\n";

        let parsed = parse(source).expect("parses");

        assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.program.statements.len(), 1);
        let comments: Vec<(&str, bool)> = parsed
            .comments()
            .iter()
            .map(|comment| {
                (
                    &*comment.text,
                    matches!(comment.kind, ast::CommentKind::Line),
                )
            })
            .collect();
        assert_eq!(comments, [(" @flow", true), (" a ", false), (" b", true)]);
    }

    #[test]
    fn reports_syntax_errors_as_diagnostics_not_failures() {
        let parsed = parse("const = ;\n").expect("the parser recovers");

        assert!(!parsed.is_ok());
        assert_eq!(parsed.diagnostics[0].line, Some(1));
    }

    #[test]
    fn parses_every_modern_flow_construct_the_port_accepts() {
        let source = r#"// @flow
export component Page(a: string, ...rest: Props) renders React.Node {
  return match (a) { "x" => <div>{a}</div>, _ => null };
}
hook useX(): number { return 1; }
enum E of string { A = "a", B = "b" }
type T<+A, -B, in C, out D, E extends string, F = number> = {| +a: A, b?: ?B, ...C |};
declare module.exports: { x: number };
const y = (x: any) as const;
"#;

        let parsed = parse(source).expect("parses");

        assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.program.statements.len(), 6);
    }

    #[test]
    fn refuses_oversized_sources_before_parsing() {
        let source = "x".repeat(MAX_PARSE_BYTES + 1);

        assert_eq!(
            parse(&source).map(|_| ()),
            Err(ParseFailure::SourceTooLarge {
                bytes: MAX_PARSE_BYTES + 1,
                limit: MAX_PARSE_BYTES,
            })
        );
    }

    #[test]
    fn refuses_nesting_past_the_ceiling() {
        let deep = "[".repeat(MAX_NESTING_DEPTH + 1) + &"]".repeat(MAX_NESTING_DEPTH + 1);

        assert_eq!(
            parse(&deep).map(|_| ()),
            Err(ParseFailure::TooDeeplyNested {
                depth: MAX_NESTING_DEPTH + 1,
                limit: MAX_NESTING_DEPTH,
            })
        );
    }

    /// The ceiling has to be one the parser actually survives on the stack
    /// callers are told to give it, or it is not a ceiling at all. Object
    /// literals are the most expensive shape per level, so they are the
    /// measurement that matters.
    #[test]
    fn survives_nesting_at_the_ceiling_on_the_documented_stack() {
        let worker = std::thread::Builder::new()
            .stack_size(PARSE_STACK_BYTES)
            .spawn(|| {
                for (open, close) in [
                    ("[", "]"),
                    ("(", ")"),
                    ("{a:", "}"),
                    ("`${", "}`"),
                    ("f(", ")"),
                ] {
                    let source = format!(
                        "x = {}1{};\n",
                        open.repeat(MAX_NESTING_DEPTH),
                        close.repeat(MAX_NESTING_DEPTH)
                    );
                    let parsed = parse(&source).expect("at the ceiling");
                    assert!(parsed.is_ok(), "{open}: {:?}", parsed.diagnostics);
                }
            })
            .expect("spawns");
        worker.join().expect("parses at the ceiling");
    }

    #[test]
    fn nesting_depth_counts_brackets_and_template_substitutions() {
        assert_eq!(nesting_depth("f(a, [b])"), 2);
        assert_eq!(nesting_depth("`${`${`${x}`}`}`"), 3);
        assert_eq!(nesting_depth("`${(\"}\", `${x}`)}`"), 3);
        assert_eq!(nesting_depth("`a${b}c${d}`"), 1);
    }

    #[test]
    fn nesting_depth_ignores_brackets_that_are_text() {
        assert_eq!(nesting_depth("f(\"{{{{\", /* [[[[ */ '(((')"), 1);
        assert_eq!(nesting_depth("x = a / b / c; // ((((\n"), 0);
        assert_eq!(nesting_depth("const r = /\\(/; const s = /[(]/;"), 0);
        assert_eq!(nesting_depth("return /(/.test(x)"), 1);
        assert_eq!(nesting_depth("`\\${(`"), 0);
    }

    #[test]
    fn nesting_depth_never_underflows() {
        assert_eq!(nesting_depth("}}}}((("), 3);
        assert_eq!(nesting_depth(")))]]]"), 0);
    }

    #[test]
    fn chain_depth_counts_operators_that_open_no_bracket() {
        // The shape that measured zero and aborted the formatter.
        assert_eq!(nesting_depth("x = 1 + 1 + 1 + 1;"), 0);
        assert_eq!(chain_depth("x = 1 + 1 + 1 + 1;"), 4);

        // A run of operator bytes is one level, not one per byte.
        assert_eq!(chain_depth("x = a === b;"), 2);
        assert_eq!(chain_depth("x = a && b && c;"), 3);

        // Member chains. With the calls it is six, not three: `.f()` is a
        // member *and* a call, and both are nodes.
        assert_eq!(chain_depth("a.b.c.d"), 3);
        assert_eq!(chain_depth("a.b().c().d()"), 6);

        // A call and an index nest even though their brackets close again.
        // `f()()()…` and `a[0][0]…` keep the bracket depth at one.
        assert_eq!(chain_depth("f()()()"), 3);
        assert_eq!(chain_depth("a[0][0][0]"), 3);
        // After an operator the same brackets are a group or an array
        // literal, which the bracket count already has.
        assert_eq!(chain_depth("x = (((a)))"), 1);
        assert_eq!(chain_depth("x = [[[a]]]"), 1);

        // A prefix operator spelled as a word nests like one spelled in
        // punctuation.
        assert_eq!(chain_depth("typeof typeof typeof x"), 3);
        assert_eq!(chain_depth("new new new Foo"), 3);
        assert_eq!(chain_depth("await await x"), 2);

        // Division is decided in the branch that also reads regular
        // expressions, so it is counted there or not at all.
        assert_eq!(chain_depth("x = a / b / c;"), 3);
        assert_eq!(chain_depth("x = /a/ + /b/;"), 2);
    }

    #[test]
    fn chain_depth_is_depth_and_not_width() {
        // Five hundred siblings are not a chain of five hundred: `,` and `;`
        // end a run, which is what keeps a wide array from measuring deep.
        // Three is the `=`, the `[` and the `+` that any one element needs.
        let wide = format!("x = [{}];", vec!["a + b"; 500].join(", "));
        assert_eq!(chain_depth(&wide), 3);
        let statements = "x = a + b;\n".repeat(500);
        assert_eq!(chain_depth(&statements), 2);

        // An inner expression is charged for what it sits in, and no more:
        // `f(a + b)` is a call, its parentheses, and a `+`.
        assert_eq!(chain_depth("f(a + b)"), 3);
        assert_eq!(chain_depth("a + f(b + c)"), 4);
    }

    #[test]
    fn a_chain_past_the_ceiling_is_refused_rather_than_overflowing() {
        let deep = MAX_CHAIN_DEPTH + 2;
        for source in [
            format!("x = {};", vec!["1"; deep].join(" + ")),
            format!("x = a{};", ".f()".repeat(deep)),
            // Each of these keeps the bracket depth at one, or opens no
            // bracket at all, and each was reaching the parser.
            format!("x = f{};", "()".repeat(deep)),
            format!("x = a{};", "[0]".repeat(deep)),
            format!("x = {}y;", "typeof ".repeat(deep)),
            format!("x = {}Foo;", "new ".repeat(deep)),
        ] {
            // Refused before the tree exists, so nothing deep is built and
            // nothing deep is freed: this one needs no stack of its own.
            let error = parse(&source).expect_err("refused");
            assert!(
                matches!(error, ParseFailure::TooDeeplyChained { .. }),
                "{error:?}"
            );
            // And it says which kind of nesting, because "brackets" about
            // `1 + 1 + …` sends whoever reads it looking for brackets.
            assert!(error.to_string().starts_with("operators chain"), "{error}");
        }
    }

    #[test]
    fn a_chain_at_the_ceiling_still_parses() {
        // On a thread the size `parse` documents: the parser recurses once
        // per level and its frames are large. Freeing the tree needs no such
        // arrangement — that is `Parsed`'s own `Drop`, and the tests below.
        std::thread::Builder::new()
            .stack_size(PARSE_STACK_BYTES)
            .spawn(|| {
                let source = format!("x = {};", vec!["1"; MAX_CHAIN_DEPTH].join(" + "));
                let parsed = parse(&source).expect("parses");
                assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
            })
            .expect("spawns")
            .join()
            .expect("no overflow at the ceiling");
    }

    /// The name of the child arm of
    /// [`a_tree_at_the_ceilings_frees_on_an_ordinary_thread`], as libtest
    /// spells it. A typo here would filter every test out and leave a child
    /// that exits 0 having run nothing, so the parent checks the count too.
    const DEEP_DROP_CHILD: &str = "parse::tests::frees_a_tree_at_the_ceilings_here_and_now";

    /// A `Parsed` at both ceilings, freed on a thread that has no room to
    /// recurse that far — ubugeeei-prod/uf#155.
    ///
    /// # Why this runs in a child process
    ///
    /// A stack overflow is not a panic. It is a guard-page fault the runtime
    /// turns into `fatal runtime error: stack overflow, aborting`, and it
    /// takes the whole process with it — `catch_unwind` does not see it and
    /// neither does `JoinHandle::join`, so a thread cannot contain it either.
    /// The only boundary that survives one is a process boundary. So this
    /// re-runs the test binary with the child arm below selected by name and
    /// reads its exit status: before `Parsed`'s `Drop` existed the child died
    /// of `SIGABRT` (134) with that message on stderr, and now it exits 0.
    #[test]
    fn a_tree_at_the_ceilings_frees_on_an_ordinary_thread() {
        let binary = std::env::current_exe().expect("the test binary's own path");
        let child = std::process::Command::new(&binary)
            .args(["--exact", "--ignored", "--nocapture", DEEP_DROP_CHILD])
            .output()
            .expect("runs the test binary again");

        let stdout = String::from_utf8_lossy(&child.stdout);
        assert!(
            child.status.success(),
            "freeing a tree at the ceilings killed the child ({}):\n{}\n{}",
            child.status,
            stdout,
            String::from_utf8_lossy(&child.stderr),
        );
        // An exit status of 0 also describes a run that matched no test at
        // all, which is what a renamed child arm would produce.
        assert!(
            stdout.contains("1 passed"),
            "the child ran no test; is {DEEP_DROP_CHILD} still the child arm's name?\n{stdout}",
        );
    }

    /// The child arm. Ignored so that only its parent runs it, and only in
    /// the process its parent starts for it.
    #[test]
    #[ignore = "aborts the process before the fix; run by its parent test"]
    fn frees_a_tree_at_the_ceilings_here_and_now() {
        for source in [
            // At `MAX_CHAIN_DEPTH`: one `=` and 9,999 `+`.
            format!("x = {};", vec!["1"; MAX_CHAIN_DEPTH].join(" + ")),
            // At `MAX_NESTING_DEPTH`, in the shape that costs the parser the
            // most per level.
            format!(
                "x = {}1{};\n",
                "{a:".repeat(MAX_NESTING_DEPTH),
                "}".repeat(MAX_NESTING_DEPTH)
            ),
        ] {
            // Built where the parser has the room it asks for...
            let parsed = std::thread::Builder::new()
                .stack_size(PARSE_STACK_BYTES)
                .spawn(move || parse(&source).expect("parses"))
                .expect("spawns")
                .join()
                .expect("parses at the ceilings");

            // ...and freed where it does not: 2 MiB, which is what a thread
            // gets when nobody chooses, and what the reproduction on the
            // issue used.
            std::thread::Builder::new()
                .stack_size(2 * 1024 * 1024)
                .spawn(move || drop(parsed))
                .expect("spawns")
                .join()
                .expect("frees on an ordinary stack");
        }
    }

    /// The other half of the fix, and the half an overflow cannot show: that
    /// ordinary source does *not* pay for any of this.
    #[test]
    fn source_anybody_writes_is_freed_where_it_lies() {
        for source in [
            "// @flow\nconst x = a.b.c().d(e ?? f);\n",
            "component Page() renders React.Node { return <ul>{xs.map((x) => <li>{x}</li>)}</ul>; }\n",
            &"const value = { a: { b: [1, 2, 3] } };\n".repeat(500),
        ] {
            let parsed = parse(source).expect("parses");
            assert!(
                !parsed.freed_off_thread(),
                "{} deep, over the {MAX_DEPTH_FREED_IN_PLACE} that is freed in place",
                parsed.depth,
            );
        }

        // And that the threshold is a threshold: one level past it goes to a
        // thread, so the comparison cannot quietly stop deciding anything.
        let deep = format!(
            "x = {};",
            vec!["1"; MAX_DEPTH_FREED_IN_PLACE + 1].join(" + ")
        );
        assert!(parse(&deep).expect("parses").freed_off_thread());
    }
}
