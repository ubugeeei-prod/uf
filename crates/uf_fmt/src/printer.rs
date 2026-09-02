//! Token-driven printer.
//!
//! The printer never reorders, drops or invents program tokens. It rewrites
//! trivia — indentation, spacing, blank lines — normalizes string quotes, and
//! adds or removes statement-terminating semicolons under a deliberately
//! conservative rule. Everything else in the output is the input.
//!
//! Layout is produced in two passes over the token stream:
//!
//! 1. [`analyze`] walks the tokens once with an explicit frame stack and records,
//!    for every token, whether a space precedes it, what indentation a line
//!    starting at it would use, and — for opening delimiters — where the group
//!    closes and how wide it would print flat.
//! 2. [`Emitter`] walks the tokens a second time and writes the output, using the
//!    recorded decisions plus the live column to explode groups that would
//!    overflow [`FmtConfig::line_width`].
//!
//! Both passes use explicit stacks, never recursion, so a source nested ten
//! thousand braces deep cannot overflow the native stack.

use uf_config::{FmtConfig, QuoteStyle};

use crate::lexer::{
    BraceKind, GroupKind, Keyword, Prev, Punctuator, Token, TokenKind, classify_brace,
    expression_allowed,
};

/// Indentation is capped so that pathological nesting cannot make the printer
/// allocate an unbounded amount of leading whitespace per line.
const MAX_INDENT_LEVELS: u16 = 256;

/// Upper bound on how many tokens a speculative type-argument scan may inspect,
/// which keeps long `a < b` chains from making the scan quadratic.
const TYPE_ARGUMENT_SCAN_BUDGET: u32 = 4096;

/// Sentinel for "this opening delimiter never closes".
const NO_MATCH: u32 = u32::MAX;

/// Per-token layout decisions produced by [`analyze`].
#[derive(Debug, Clone, Copy)]
struct Anno {
    /// Whether a single space separates this token from the previous one.
    space_before: bool,
    /// Whether this opening delimiter may be exploded across several lines.
    breakable: bool,
    /// Whether this whitespace token is significant JSX character data.
    jsx_space: bool,
    /// Whether this `)` closes an `if`/`for`/`while`/`catch`/`switch` header.
    statement_paren: bool,
    /// Whether this opening delimiter encloses statements rather than operands.
    statement_group: bool,
    /// Whether this `}` closes an object literal or object type, which may end a
    /// statement, rather than a block, which may not.
    object_close: bool,
    /// Whether this token sits inside a type-argument list such as `Array<T>`.
    in_angle: bool,
    /// Whether this token is still inside a JSX element after it is printed.
    in_jsx: bool,
    /// Printed width of the token, measured from its last line.
    width: u32,
    /// Whether the token's text spans more than one line.
    multiline: bool,
    /// Indentation level to use when this token starts a line.
    indent: u16,
    /// Index of the matching closing delimiter, or [`NO_MATCH`].
    close: u32,
}

impl Default for Anno {
    fn default() -> Self {
        Self {
            space_before: false,
            breakable: false,
            jsx_space: false,
            statement_paren: false,
            statement_group: false,
            object_close: false,
            in_angle: false,
            in_jsx: false,
            width: 0,
            multiline: false,
            indent: 0,
            close: NO_MATCH,
        }
    }
}

/// Everything the emit pass needs to know about the token stream.
struct Analysis {
    annos: Vec<Anno>,
    /// Prefix sums of the flat printed width of each token.
    cost: Vec<u32>,
    /// For each index, the index of the next newline token at or after it.
    line_end: Vec<u32>,
}

/// Where a token sits relative to a type-argument list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Angle {
    /// Not part of a type-argument list.
    No,
    /// The `<` or `>` of a type-argument list.
    Bracket,
    /// A token nested inside a type-argument list.
    Inside,
}

/// The syntactic role of a token, which decides the spacing around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Normal,
    /// A prefix operator: `!x`, `-1`, `...rest`, `?number`, `+variance`.
    Prefix,
    /// The `<` that opens a type-argument list.
    TypeAngleOpen,
    /// The `>` (or `>>`) that closes a type-argument list.
    TypeAngleClose,
    /// The `:` of a conditional expression.
    TernaryColon,
    /// The `?` of `name?: T` or `name?,`.
    OptionalMarker,
}

/// A bracketed region tracked while analysing.
#[derive(Debug, Clone, Copy)]
struct Frame {
    kind: FrameKind,
    open: u32,
    ternary_depth: u32,
    /// A newline occurred somewhere inside this group.
    has_newline: bool,
    /// A newline occurred at this group's own level rather than inside a nested
    /// group, which is what earns the group an indentation level.
    has_own_newline: bool,
    has_separator: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    Paren { statement_header: bool },
    Bracket,
    Brace(BraceKind),
    Template,
    JsxTag { closing: bool },
    JsxChildren,
}

impl FrameKind {
    /// Whether `{ … }` of this flavour keeps a space inside its braces.
    const fn pads_braces(self) -> bool {
        matches!(
            self,
            FrameKind::Brace(BraceKind::Block | BraceKind::Object | BraceKind::Class)
        )
    }

    /// Whether the region holds statements, which may take a semicolon.
    const fn holds_statements(self) -> bool {
        matches!(
            self,
            FrameKind::Brace(BraceKind::Block | BraceKind::Class | BraceKind::Switch)
        )
    }

    const fn is_jsx_children(self) -> bool {
        matches!(self, FrameKind::JsxChildren)
    }

    const fn is_jsx_tag(self) -> bool {
        matches!(self, FrameKind::JsxTag { .. })
    }

    const fn is_jsx(self) -> bool {
        self.is_jsx_tag() || self.is_jsx_children()
    }
}

/// Lay `tokens` out as freshly formatted source text.
pub(crate) fn print(source: &str, tokens: &[Token], config: &FmtConfig) -> String {
    let analysis = analyze(source, tokens);
    Emitter::new(source, tokens, &analysis, config).run()
}

// ---------------------------------------------------------------- type angles

/// Mark the tokens that belong to a type-argument list such as `Array<Map<K, V>>`.
///
/// Angle brackets are the one place a token-driven formatter has to guess: `a < b`
/// is a comparison and `A<B>` is a type application, and the two are only told
/// apart by what surrounds them. The scan therefore starts only after an
/// identifier, accepts only type-shaped tokens, and commits only when the token
/// after the closing `>` is one that may legally follow a type.
fn mark_type_angles(tokens: &[Token]) -> Vec<Angle> {
    let mut marks = vec![Angle::No; tokens.len()];
    let mut prev: Option<TokenKind> = None;
    let mut index = 0;

    while index < tokens.len() {
        let kind = tokens[index].kind;
        if kind.is_trivia() {
            index += 1;
            continue;
        }

        if kind == TokenKind::Punctuator(Punctuator::Less)
            && matches!(prev, Some(TokenKind::Identifier))
            && let Some(close) = scan_type_arguments(tokens, index)
        {
            for (offset, mark) in marks.iter_mut().enumerate().take(close + 1).skip(index) {
                let is_bracket = matches!(
                    tokens[offset].kind,
                    TokenKind::Punctuator(
                        Punctuator::Less
                            | Punctuator::Greater
                            | Punctuator::GreaterGreater
                            | Punctuator::GreaterGreaterGreater
                    )
                );
                *mark = if is_bracket {
                    Angle::Bracket
                } else {
                    Angle::Inside
                };
            }
            prev = Some(tokens[close].kind);
            index = close + 1;
            continue;
        }

        prev = Some(kind);
        index += 1;
    }

    marks
}

/// Find the `>` that closes the type-argument list opening at `start`.
fn scan_type_arguments(tokens: &[Token], start: usize) -> Option<usize> {
    let mut depth: u32 = 1;
    let mut budget = TYPE_ARGUMENT_SCAN_BUDGET;
    let mut index = start + 1;
    let mut saw_content = false;

    while index < tokens.len() {
        let kind = tokens[index].kind;
        if kind.is_trivia() {
            index += 1;
            continue;
        }
        if budget == 0 {
            return None;
        }
        budget -= 1;

        match kind {
            TokenKind::Identifier | TokenKind::Number | TokenKind::String => saw_content = true,
            TokenKind::Keyword(
                Keyword::Typeof
                | Keyword::Void
                | Keyword::Null
                | Keyword::True
                | Keyword::False
                | Keyword::This
                | Keyword::Static
                | Keyword::Interface,
            ) => saw_content = true,
            TokenKind::Punctuator(punctuator) => match punctuator {
                Punctuator::Less => {
                    depth += 1;
                    saw_content = true;
                }
                Punctuator::Greater
                | Punctuator::GreaterGreater
                | Punctuator::GreaterGreaterGreater => {
                    let closes = punctuator.angle_close_count();
                    if closes > depth {
                        return None;
                    }
                    depth -= closes;
                    if depth == 0 {
                        if !saw_content {
                            return None;
                        }
                        return type_arguments_may_precede(tokens, index).then_some(index);
                    }
                }
                Punctuator::Dot
                | Punctuator::Comma
                | Punctuator::Pipe
                | Punctuator::Amp
                | Punctuator::Question
                | Punctuator::Colon
                | Punctuator::OpenBracket
                | Punctuator::CloseBracket
                | Punctuator::OpenBrace
                | Punctuator::CloseBrace
                | Punctuator::OpenParen
                | Punctuator::CloseParen
                | Punctuator::Arrow
                | Punctuator::Ellipsis
                | Punctuator::Star
                | Punctuator::Plus
                | Punctuator::Minus
                | Punctuator::Equal => saw_content = true,
                _ => return None,
            },
            _ => return None,
        }

        index += 1;
    }

    None
}

/// Whether the token after a closing `>` is compatible with a type application.
fn type_arguments_may_precede(tokens: &[Token], close: usize) -> bool {
    let Some(next) = tokens[close + 1..]
        .iter()
        .find(|token| !token.kind.is_trivia())
    else {
        return true;
    };

    match next.kind {
        TokenKind::Punctuator(punctuator) => matches!(
            punctuator,
            Punctuator::OpenParen
                | Punctuator::CloseParen
                | Punctuator::OpenBrace
                | Punctuator::CloseBrace
                | Punctuator::OpenBracket
                | Punctuator::CloseBracket
                | Punctuator::Comma
                | Punctuator::Semicolon
                | Punctuator::Colon
                | Punctuator::Equal
                | Punctuator::Arrow
                | Punctuator::Pipe
                | Punctuator::Amp
                | Punctuator::Question
                | Punctuator::Dot
                | Punctuator::Ellipsis
                | Punctuator::Greater
                | Punctuator::GreaterGreater
                | Punctuator::GreaterGreaterGreater
        ),
        TokenKind::TemplateFull | TokenKind::TemplateHead => true,
        TokenKind::Keyword(keyword) => matches!(
            keyword,
            Keyword::Extends | Keyword::Implements | Keyword::From | Keyword::Renders
        ),
        _ => false,
    }
}

// -------------------------------------------------------------------- analyze

/// Walk the tokens once and record every layout decision that does not depend on
/// the output column.
#[allow(
    clippy::too_many_lines,
    reason = "one linear pass over the token kinds"
)]
fn analyze(source: &str, tokens: &[Token]) -> Analysis {
    let count = tokens.len();
    let angles = mark_type_angles(tokens);
    let mut annos = vec![Anno::default(); count];
    let mut cost = vec![0u32; count + 1];
    let mut line_end = vec![u32::try_from(count).unwrap_or(u32::MAX); count + 1];

    let mut frames: Vec<Frame> = Vec::with_capacity(16);
    let mut brace_stack: Vec<BraceKind> = Vec::with_capacity(16);
    let mut indent_level: u16 = 0;
    let mut jsx_depth: u32 = 0;
    let mut root_ternary: u32 = 0;
    let mut switch_depth: u16 = 0;
    let mut prev: Option<Prev> = None;
    let mut prev_role = Role::Normal;
    let mut after_comment = false;
    let mut pending_body: Option<BraceKind> = None;

    for index in 0..count {
        let token = tokens[index];
        let kind = token.kind;
        let text = token.text(source);
        let frame = frames.last().copied();

        if kind == TokenKind::Newline {
            if let Some(frame) = frames.last_mut()
                && !frame.has_own_newline
            {
                frame.has_own_newline = true;
                indent_level = indent_level.saturating_add(1);
            }
            cost[index + 1] = cost[index];
            after_comment = false;
            continue;
        }

        let width = display_width(text);
        annos[index].width = width;
        if kind == TokenKind::Whitespace {
            let jsx_space = frame.is_some_and(|frame| frame.kind.is_jsx_children());
            annos[index].jsx_space = jsx_space;
            cost[index + 1] = cost[index] + if jsx_space { width } else { 0 };
            continue;
        }

        // A token that spans lines (a multi-line template or block comment) keeps
        // its group from being reflowed, but does not indent anything.
        if memchr::memchr(b'\n', text.as_bytes()).is_some() {
            annos[index].multiline = true;
            if let Some(frame) = frames.last_mut() {
                frame.has_newline = true;
            }
        }

        let next_significant = tokens[index + 1..]
            .iter()
            .find(|token| !token.kind.is_trivia())
            .map(|token| token.kind);
        let ternary_depth = frames
            .last()
            .map_or(root_ternary, |frame| frame.ternary_depth);
        let role = role_of(
            tokens,
            index,
            prev,
            next_significant,
            ternary_depth,
            &angles,
        );
        let space_before = if prev.is_none() {
            false
        } else {
            (after_comment && !hugs_left(kind)) || wants_space(prev, prev_role, kind, role, frame)
        };

        annos[index].space_before = space_before;
        annos[index].in_angle = angles[index] != Angle::No;
        annos[index].indent = indent_for(kind, next_significant, frame, indent_level, switch_depth);
        cost[index + 1] = cost[index]
            .saturating_add(width)
            .saturating_add(u32::from(space_before));

        // A separator only counts as "top level" when neither a nested group nor
        // a type-argument list stands between it and the enclosing delimiter.
        if matches!(
            kind,
            TokenKind::Punctuator(Punctuator::Comma | Punctuator::Semicolon)
        ) && angles[index] == Angle::No
            && let Some(frame) = frames.last_mut()
        {
            frame.has_separator = true;
        }

        let mut group = GroupKind::None;
        match kind {
            TokenKind::Punctuator(Punctuator::OpenParen) => {
                let statement_header = matches!(
                    prev.map(|prev| prev.kind),
                    Some(TokenKind::Keyword(keyword)) if keyword.starts_statement_header()
                );
                push_frame(&mut frames, FrameKind::Paren { statement_header }, index);
            }
            TokenKind::Punctuator(Punctuator::OpenBracket) => {
                push_frame(&mut frames, FrameKind::Bracket, index);
            }
            TokenKind::Punctuator(Punctuator::OpenBrace) => {
                let brace = match frame.map(|frame| frame.kind) {
                    Some(kind) if kind.is_jsx() => BraceKind::JsxExpression,
                    _ => pending_body
                        .take()
                        .unwrap_or_else(|| classify_brace(prev, brace_stack.last().copied())),
                };
                brace_stack.push(brace);
                if brace == BraceKind::Switch {
                    switch_depth = switch_depth.saturating_add(1);
                }
                push_frame(&mut frames, FrameKind::Brace(brace), index);
            }
            TokenKind::Punctuator(Punctuator::CloseParen) => {
                if let Some(frame) = pop_frame(&mut frames, &mut indent_level, &mut jsx_depth) {
                    annos[index].statement_paren = matches!(
                        frame.kind,
                        FrameKind::Paren {
                            statement_header: true
                        }
                    );
                    close_group(&mut annos, &frame, index, &angles);
                }
                group = if annos[index].statement_paren {
                    GroupKind::StatementParen
                } else {
                    GroupKind::ExpressionParen
                };
            }
            TokenKind::Punctuator(Punctuator::CloseBracket) => {
                if let Some(frame) = pop_frame(&mut frames, &mut indent_level, &mut jsx_depth) {
                    close_group(&mut annos, &frame, index, &angles);
                }
            }
            TokenKind::Punctuator(Punctuator::CloseBrace) => {
                let brace = brace_stack.pop().unwrap_or(BraceKind::Block);
                if brace == BraceKind::Switch {
                    switch_depth = switch_depth.saturating_sub(1);
                }
                annos[index].object_close = brace == BraceKind::Object;
                if let Some(frame) = pop_frame(&mut frames, &mut indent_level, &mut jsx_depth) {
                    close_group(&mut annos, &frame, index, &angles);
                }
                group = GroupKind::Brace(brace);
            }
            TokenKind::TemplateHead => {
                push_frame(&mut frames, FrameKind::Template, index);
            }
            TokenKind::TemplateTail => {
                pop_frame(&mut frames, &mut indent_level, &mut jsx_depth);
            }
            TokenKind::JsxOpenStart => {
                jsx_depth += 1;
                push_frame(&mut frames, FrameKind::JsxTag { closing: false }, index);
            }
            TokenKind::JsxCloseStart => {
                jsx_depth += 1;
                push_frame(&mut frames, FrameKind::JsxTag { closing: true }, index);
            }
            TokenKind::JsxTagEnd => {
                let closing = matches!(
                    frames.last().map(|frame| frame.kind),
                    Some(FrameKind::JsxTag { closing: true })
                );
                pop_frame(&mut frames, &mut indent_level, &mut jsx_depth);
                if closing {
                    pop_frame(&mut frames, &mut indent_level, &mut jsx_depth);
                } else {
                    jsx_depth += 1;
                    push_frame(&mut frames, FrameKind::JsxChildren, index);
                }
            }
            TokenKind::JsxSelfClose => {
                pop_frame(&mut frames, &mut indent_level, &mut jsx_depth);
            }
            TokenKind::Keyword(keyword) => match keyword {
                Keyword::Class | Keyword::Interface | Keyword::Enum => {
                    pending_body = Some(BraceKind::Class);
                }
                Keyword::Switch => pending_body = Some(BraceKind::Switch),
                _ => {}
            },
            _ => {}
        }

        annos[index].in_jsx = jsx_depth > 0;

        if role == Role::TernaryColon {
            match frames.last_mut() {
                Some(frame) => frame.ternary_depth = frame.ternary_depth.saturating_sub(1),
                None => root_ternary = root_ternary.saturating_sub(1),
            }
        } else if kind == TokenKind::Punctuator(Punctuator::Question)
            && role == Role::Normal
            && angles[index] == Angle::No
        {
            match frames.last_mut() {
                Some(frame) => frame.ternary_depth = frame.ternary_depth.saturating_add(1),
                None => root_ternary = root_ternary.saturating_add(1),
            }
        }

        if kind.is_comment() {
            after_comment = true;
        } else {
            after_comment = false;
            prev = Some(Prev { kind, group });
            prev_role = role;
        }
    }

    for index in (0..count).rev() {
        line_end[index] = if tokens[index].kind == TokenKind::Newline {
            u32::try_from(index).unwrap_or(u32::MAX)
        } else {
            line_end[index + 1]
        };
    }

    Analysis {
        annos,
        cost,
        line_end,
    }
}

fn push_frame(frames: &mut Vec<Frame>, kind: FrameKind, open: usize) {
    frames.push(Frame {
        kind,
        open: u32::try_from(open).unwrap_or(NO_MATCH),
        ternary_depth: 0,
        has_newline: false,
        has_own_newline: false,
        has_separator: false,
    });
}

/// Pop the innermost frame, if any.
///
/// Unbalanced sources are formatted rather than rejected, so a stray closing
/// delimiter simply closes whatever frame happens to be open.
fn pop_frame(
    frames: &mut Vec<Frame>,
    indent_level: &mut u16,
    jsx_depth: &mut u32,
) -> Option<Frame> {
    let frame = frames.pop()?;
    if frame.kind.is_jsx() {
        *jsx_depth = jsx_depth.saturating_sub(1);
    }
    if frame.has_own_newline {
        *indent_level = indent_level.saturating_sub(1);
    }
    // Newlines inside a closed group still count as newlines inside its parent,
    // which is what makes the parent unbreakable.
    if let Some(parent) = frames.last_mut() {
        parent.has_newline |= frame.has_newline || frame.has_own_newline;
    }
    Some(frame)
}

fn close_group(annos: &mut [Anno], frame: &Frame, close: usize, angles: &[Angle]) {
    let open = frame.open as usize;
    if open >= annos.len() {
        return;
    }
    annos[open].close = u32::try_from(close).unwrap_or(NO_MATCH);
    annos[open].statement_group = frame.kind.holds_statements();
    annos[open].breakable = frame.has_separator
        && !frame.has_newline
        && !frame.has_own_newline
        && !frame.kind.is_jsx()
        && angles[open] == Angle::No;
}

/// Indentation level for a line that starts with `kind`.
fn indent_for(
    kind: TokenKind,
    next: Option<TokenKind>,
    frame: Option<Frame>,
    indent_level: u16,
    switch_depth: u16,
) -> u16 {
    let top = frame.map(|frame| frame.kind);
    let dedents = match kind {
        TokenKind::Punctuator(punctuator) if punctuator.is_close_delimiter() => true,
        // The trailing `|` of a Flow exact object type belongs to the closing
        // brace, not to the body.
        TokenKind::Punctuator(Punctuator::Pipe) => {
            next == Some(TokenKind::Punctuator(Punctuator::CloseBrace))
        }
        TokenKind::JsxTagEnd | TokenKind::JsxSelfClose => top.is_some_and(FrameKind::is_jsx_tag),
        TokenKind::JsxCloseStart => top.is_some_and(FrameKind::is_jsx_children),
        TokenKind::TemplateMiddle | TokenKind::TemplateTail => {
            matches!(top, Some(FrameKind::Template))
        }
        _ => false,
    };

    let innermost_is_switch = matches!(top, Some(FrameKind::Brace(BraceKind::Switch)));
    // Statements inside a `switch` body sit one level deeper than its `case` and
    // `default` labels, and the offset accumulates through nested switches.
    let mut level = indent_level.saturating_add(switch_depth);
    if innermost_is_switch
        && (dedents || matches!(kind, TokenKind::Keyword(Keyword::Case | Keyword::Default)))
    {
        level = level.saturating_sub(1);
    }

    if dedents {
        // Only groups that broke at their own level contributed a level to undo.
        if frame.is_some_and(|frame| frame.has_own_newline) {
            level = level.saturating_sub(1);
        }
    } else if matches!(
        kind,
        TokenKind::Punctuator(Punctuator::Dot | Punctuator::QuestionDot)
    ) {
        // A line that starts with `.` continues the previous expression.
        level = level.saturating_add(1);
    }
    level.min(MAX_INDENT_LEVELS)
}

/// Resolve the syntactic role of the token at `index`.
fn role_of(
    tokens: &[Token],
    index: usize,
    prev: Option<Prev>,
    next: Option<TokenKind>,
    ternary_depth: u32,
    angles: &[Angle],
) -> Role {
    let kind = tokens[index].kind;

    if angles[index] == Angle::Bracket {
        return if kind == TokenKind::Punctuator(Punctuator::Less) {
            Role::TypeAngleOpen
        } else {
            Role::TypeAngleClose
        };
    }

    let TokenKind::Punctuator(punctuator) = kind else {
        return Role::Normal;
    };

    match punctuator {
        Punctuator::Bang | Punctuator::Tilde | Punctuator::Ellipsis | Punctuator::At => {
            Role::Prefix
        }
        // `function* gen()` and `yield* other()` keep the star attached on the
        // left instead, so they are not prefix operators.
        Punctuator::Star
            if matches!(
                prev.map(|prev| prev.kind),
                Some(TokenKind::Keyword(Keyword::Function | Keyword::Yield))
            ) =>
        {
            Role::Normal
        }
        Punctuator::Plus
        | Punctuator::Minus
        | Punctuator::PlusPlus
        | Punctuator::MinusMinus
        | Punctuator::Star
            if expression_allowed(prev) =>
        {
            Role::Prefix
        }
        Punctuator::Colon => {
            if ternary_depth > 0 {
                Role::TernaryColon
            } else {
                Role::Normal
            }
        }
        Punctuator::Question => {
            let optional = matches!(
                next,
                Some(TokenKind::Punctuator(
                    Punctuator::Colon
                        | Punctuator::Comma
                        | Punctuator::CloseParen
                        | Punctuator::CloseBracket
                        | Punctuator::CloseBrace
                        | Punctuator::Equal
                ))
            );
            if optional {
                return Role::OptionalMarker;
            }
            // `?string`, `Array<?T>` and `(x: ?number)` are nullable types, so the
            // `?` is a prefix rather than the head of a conditional expression.
            let prefix = matches!(
                prev.map(|prev| prev.kind),
                Some(TokenKind::Punctuator(
                    Punctuator::Colon
                        | Punctuator::Equal
                        | Punctuator::Pipe
                        | Punctuator::Amp
                        | Punctuator::Comma
                        | Punctuator::OpenParen
                        | Punctuator::OpenBracket
                        | Punctuator::Less
                        | Punctuator::Arrow
                        | Punctuator::Question
                        | Punctuator::Ellipsis
                ))
            );
            if prefix { Role::Prefix } else { Role::Normal }
        }
        _ => Role::Normal,
    }
}

/// Whether a single space separates `prev` from the current token.
#[allow(clippy::too_many_lines, reason = "a flat table of spacing rules")]
fn wants_space(
    prev: Option<Prev>,
    prev_role: Role,
    kind: TokenKind,
    role: Role,
    frame: Option<Frame>,
) -> bool {
    let Some(prev) = prev else {
        return false;
    };
    let prev_kind = prev.kind;
    let frame_kind = frame.map(|frame| frame.kind);

    // JSX character data carries its own whitespace verbatim.
    if frame_kind.is_some_and(FrameKind::is_jsx_children) {
        return false;
    }
    if frame_kind.is_some_and(FrameKind::is_jsx_tag) {
        return jsx_tag_space(prev_kind, kind);
    }

    // Flow exact object types: `{| … |}`.
    if prev_kind == TokenKind::Punctuator(Punctuator::OpenBrace)
        && kind == TokenKind::Punctuator(Punctuator::Pipe)
    {
        return false;
    }
    if prev_kind == TokenKind::Punctuator(Punctuator::Pipe)
        && kind == TokenKind::Punctuator(Punctuator::CloseBrace)
    {
        return false;
    }

    if matches!(
        prev_kind,
        TokenKind::Punctuator(Punctuator::OpenParen | Punctuator::OpenBracket)
    ) {
        return false;
    }
    if matches!(
        kind,
        TokenKind::Punctuator(Punctuator::CloseParen | Punctuator::CloseBracket)
    ) {
        return false;
    }

    if prev_kind == TokenKind::Punctuator(Punctuator::OpenBrace) {
        if kind == TokenKind::Punctuator(Punctuator::CloseBrace) {
            return false;
        }
        return frame_kind.is_some_and(FrameKind::pads_braces);
    }
    if kind == TokenKind::Punctuator(Punctuator::CloseBrace) {
        return frame_kind.is_some_and(FrameKind::pads_braces);
    }

    if matches!(prev_role, Role::Prefix | Role::TypeAngleOpen) {
        return false;
    }
    if matches!(
        role,
        Role::TypeAngleOpen | Role::TypeAngleClose | Role::OptionalMarker
    ) {
        return false;
    }
    // `useState<number>(0)` and `Array<T>[]` keep the call or index attached to
    // the closing angle bracket; everything else after `>` is spaced normally.
    if prev_role == Role::TypeAngleClose
        && matches!(
            kind,
            TokenKind::Punctuator(Punctuator::OpenParen | Punctuator::OpenBracket)
                | TokenKind::TemplateFull
                | TokenKind::TemplateHead
        )
    {
        return false;
    }
    if matches!(
        prev_kind,
        TokenKind::Punctuator(Punctuator::Dot | Punctuator::QuestionDot)
            | TokenKind::TemplateHead
            | TokenKind::TemplateMiddle
    ) {
        return false;
    }

    match kind {
        TokenKind::Punctuator(
            Punctuator::Comma | Punctuator::Semicolon | Punctuator::Dot | Punctuator::QuestionDot,
        ) => false,
        TokenKind::Punctuator(Punctuator::Colon) => role == Role::TernaryColon,
        TokenKind::TemplateMiddle | TokenKind::TemplateTail => false,
        TokenKind::Punctuator(Punctuator::OpenParen) => !callable_prefix(prev_kind),
        TokenKind::Punctuator(Punctuator::OpenBracket)
        | TokenKind::TemplateFull
        | TokenKind::TemplateHead
        | TokenKind::Punctuator(Punctuator::PlusPlus | Punctuator::MinusMinus) => {
            !indexable_prefix(prev_kind)
        }
        TokenKind::Punctuator(Punctuator::Star) => !matches!(
            prev_kind,
            TokenKind::Keyword(Keyword::Function | Keyword::Yield)
        ),
        _ => true,
    }
}

/// Whether a token always sits flush against whatever precedes it, so that a
/// preceding comment does not force a space in front of it.
fn hugs_left(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Punctuator(
            Punctuator::CloseParen
                | Punctuator::CloseBracket
                | Punctuator::Comma
                | Punctuator::Semicolon
                | Punctuator::Dot
                | Punctuator::QuestionDot
        ) | TokenKind::JsxTagEnd
            | TokenKind::JsxSelfClose
    )
}

/// Spacing rules inside a JSX tag, where `=` binds attributes tightly.
fn jsx_tag_space(prev_kind: TokenKind, kind: TokenKind) -> bool {
    if matches!(
        prev_kind,
        TokenKind::JsxOpenStart | TokenKind::JsxCloseStart
    ) {
        return false;
    }
    if kind == TokenKind::JsxTagEnd {
        return false;
    }
    if kind == TokenKind::Punctuator(Punctuator::Equal)
        || prev_kind == TokenKind::Punctuator(Punctuator::Equal)
    {
        return false;
    }
    if prev_kind == TokenKind::Punctuator(Punctuator::OpenBrace)
        || kind == TokenKind::Punctuator(Punctuator::CloseBrace)
        || prev_kind == TokenKind::Punctuator(Punctuator::Slash)
    {
        return false;
    }
    true
}

/// Whether `(` directly after this token is a call rather than a grouping.
fn callable_prefix(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier
            | TokenKind::PrivateName
            | TokenKind::Punctuator(Punctuator::CloseParen | Punctuator::CloseBracket)
            | TokenKind::Keyword(Keyword::Super | Keyword::Import)
    )
}

/// Whether `[` directly after this token is an index rather than a literal.
fn indexable_prefix(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier
            | TokenKind::PrivateName
            | TokenKind::Number
            | TokenKind::String
            | TokenKind::TemplateFull
            | TokenKind::TemplateTail
            | TokenKind::Punctuator(Punctuator::CloseParen | Punctuator::CloseBracket)
            | TokenKind::Keyword(Keyword::This | Keyword::Super)
    )
}

/// Printed width of a token, measured from the last line it covers.
fn display_width(text: &str) -> u32 {
    let tail = match memchr::memrchr(b'\n', text.as_bytes()) {
        Some(offset) => &text[offset + 1..],
        None => text,
    };
    // Source is overwhelmingly ASCII, where the byte length is the width.
    let width = if tail.is_ascii() {
        tail.len()
    } else {
        tail.chars().count()
    };
    u32::try_from(width).unwrap_or(u32::MAX)
}

// ----------------------------------------------------------------- emit pass

/// A bracketed group that is currently open in the output.
#[derive(Debug, Clone, Copy)]
struct OpenGroup {
    close: u32,
    broken: bool,
    statements: bool,
}

struct Emitter<'a> {
    source: &'a str,
    tokens: &'a [Token],
    analysis: &'a Analysis,
    indent_width: usize,
    line_width: u32,
    quotes: QuoteStyle,
    semicolons: bool,
    max_blank_lines: usize,
    out: String,
    indent_cache: String,
    column: u32,
    pending_newlines: usize,
    pending_jsx_space: Option<usize>,
    groups: Vec<OpenGroup>,
    /// How many currently open groups the printer exploded itself. The analysis
    /// pass could not know about those, so their indentation is added here.
    broken_depth: u16,
    prev_emitted: Option<usize>,
    prev_significant: Option<usize>,
    started: bool,
}

impl<'a> Emitter<'a> {
    fn new(
        source: &'a str,
        tokens: &'a [Token],
        analysis: &'a Analysis,
        config: &FmtConfig,
    ) -> Self {
        Self {
            source,
            tokens,
            analysis,
            indent_width: usize::from(config.indent_width),
            line_width: u32::from(config.line_width),
            quotes: config.quotes,
            semicolons: config.semicolons,
            max_blank_lines: usize::from(config.max_blank_lines),
            out: String::with_capacity(source.len() + source.len() / 8 + 16),
            indent_cache: String::new(),
            column: 0,
            pending_newlines: 0,
            pending_jsx_space: None,
            groups: Vec::with_capacity(16),
            broken_depth: 0,
            prev_emitted: None,
            prev_significant: None,
            started: false,
        }
    }

    fn run(mut self) -> String {
        for index in 0..self.tokens.len() {
            match self.tokens[index].kind {
                TokenKind::Newline => {
                    self.pending_newlines += 1;
                    self.pending_jsx_space = None;
                }
                TokenKind::Whitespace => {
                    if self.analysis.annos[index].jsx_space && self.pending_newlines == 0 {
                        self.pending_jsx_space = Some(index);
                    }
                }
                _ => self.emit(index),
            }
        }

        if self.semicolons && self.needs_semicolon(None) {
            self.out.push(';');
        }
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
        self.out
    }

    /// Append text whose printed width is already known.
    fn write_measured(&mut self, text: &str, width: u32, multiline: bool) {
        self.out.push_str(text);
        self.column = if multiline {
            width
        } else {
            self.column.saturating_add(width)
        };
    }

    fn write(&mut self, text: &str) {
        let multiline = memchr::memchr(b'\n', text.as_bytes()).is_some();
        self.write_measured(text, display_width(text), multiline);
    }

    fn write_indent(&mut self, level: u16) {
        let width = usize::from(level) * self.indent_width;
        while self.indent_cache.len() < width {
            self.indent_cache.push(' ');
        }
        self.out.push_str(&self.indent_cache[..width]);
        self.column = u32::try_from(width).unwrap_or(u32::MAX);
    }

    fn emit(&mut self, index: usize) {
        let token = self.tokens[index];

        if token.kind == TokenKind::Punctuator(Punctuator::Semicolon)
            && !self.semicolons
            && self.can_drop_semicolon(index)
        {
            return;
        }

        self.open_line(index);

        let text = token.text(self.source);
        let rewritten = match token.kind {
            TokenKind::String => requote(text, self.quotes),
            TokenKind::JsxString => requote_jsx(text, self.quotes),
            _ => None,
        };
        match rewritten {
            Some(rewritten) => self.write(&rewritten),
            None => {
                let anno = self.analysis.annos[index];
                self.write_measured(text, anno.width, anno.multiline);
            }
        }

        self.prev_emitted = Some(index);
        if !token.kind.is_comment() {
            self.prev_significant = Some(index);
        }

        self.update_groups(index);
    }

    /// Emit whatever separates this token from the previous one: newlines and
    /// indentation, a single space, or nothing at all.
    fn open_line(&mut self, index: usize) {
        let anno = self.analysis.annos[index];

        if !self.started {
            self.started = true;
            self.pending_newlines = 0;
            self.pending_jsx_space = None;
            let indent = self.indent_of(index);
            if indent > 0 {
                self.write_indent(indent);
            }
            return;
        }

        if self.pending_newlines > 0 {
            if self.semicolons && self.needs_semicolon(Some(index)) {
                self.out.push(';');
                self.column = self.column.saturating_add(1);
            }
            let mut count = self.pending_newlines.min(self.max_blank_lines + 1);
            if self.hugs_a_delimiter(index) {
                count = 1;
            }
            for _ in 0..count {
                self.out.push('\n');
            }
            self.column = 0;
            self.pending_newlines = 0;
            self.pending_jsx_space = None;
            let indent = self.indent_of(index);
            self.write_indent(indent);
            return;
        }

        if let Some(space) = self.pending_jsx_space.take() {
            let text = self.tokens[space].text(self.source);
            self.write(text);
            return;
        }

        if anno.space_before {
            self.out.push(' ');
            self.column = self.column.saturating_add(1);
        }
    }

    /// Indentation for a line starting at `index`, including the levels added by
    /// groups this printer exploded on its own.
    fn indent_of(&self, index: usize) -> u16 {
        let mut extra = self.broken_depth;
        if self
            .groups
            .last()
            .is_some_and(|group| group.broken && group.close as usize == index)
        {
            extra = extra.saturating_sub(1);
        }
        self.analysis.annos[index]
            .indent
            .saturating_add(extra)
            .min(MAX_INDENT_LEVELS)
    }

    /// Blank lines are dropped directly inside a delimiter pair.
    fn hugs_a_delimiter(&self, index: usize) -> bool {
        let opens = matches!(
            self.prev_emitted.map(|prev| self.tokens[prev].kind),
            Some(TokenKind::Punctuator(punctuator)) if punctuator.is_open_delimiter()
        );
        let closes = matches!(
            self.tokens[index].kind,
            TokenKind::Punctuator(punctuator) if punctuator.is_close_delimiter()
        );
        opens || closes
    }

    fn update_groups(&mut self, index: usize) {
        let kind = self.tokens[index].kind;
        let anno = self.analysis.annos[index];

        if matches!(kind, TokenKind::Punctuator(punctuator) if punctuator.is_close_delimiter()) {
            if self
                .groups
                .last()
                .is_some_and(|group| group.close as usize == index)
                && let Some(group) = self.groups.pop()
                && group.broken
            {
                self.broken_depth = self.broken_depth.saturating_sub(1);
            }
        } else if matches!(kind, TokenKind::Punctuator(punctuator) if punctuator.is_open_delimiter())
        {
            let broken = anno.breakable && anno.close != NO_MATCH && !self.group_fits(index, anno);
            self.groups.push(OpenGroup {
                close: anno.close,
                broken,
                statements: anno.statement_group,
            });
            if broken {
                self.broken_depth = self.broken_depth.saturating_add(1);
                self.pending_newlines = 1;
            }
        } else if matches!(
            kind,
            TokenKind::Punctuator(Punctuator::Comma | Punctuator::Semicolon)
        ) && !anno.in_angle
            && self.groups.last().is_some_and(|group| group.broken)
        {
            // Inside a group we exploded ourselves, every top-level separator
            // starts a new line.
            self.pending_newlines = 1;
        }

        if let Some(group) = self.groups.last()
            && group.broken
            && group.close != NO_MATCH
            && self.next_significant(index) == Some(group.close as usize)
        {
            self.pending_newlines = self.pending_newlines.max(1);
        }
    }

    /// Whether the group opening at `index` still fits on the current line.
    fn group_fits(&self, index: usize, anno: Anno) -> bool {
        if self.line_width == 0 {
            return true;
        }
        let close = anno.close as usize;
        let stop = usize::min(
            self.analysis.line_end[close] as usize,
            self.analysis.cost.len() - 1,
        );
        let span = self.analysis.cost[stop].saturating_sub(self.analysis.cost[index + 1]);
        self.column.saturating_add(span) <= self.line_width
    }

    fn next_significant(&self, index: usize) -> Option<usize> {
        self.tokens[index + 1..]
            .iter()
            .position(|token| !token.kind.is_trivia())
            .map(|offset| index + 1 + offset)
    }

    /// Whether a statement-terminating semicolon should be written before the
    /// token at `next`, which is `None` at the end of the file.
    ///
    /// The rule is deliberately conservative. Automatic semicolon insertion only
    /// fires when the next line cannot continue the current expression, so a next
    /// token such as `(`, `[`, `` ` `` or an operator blocks insertion: writing a
    /// semicolon there would change what the program means.
    fn needs_semicolon(&self, next: Option<usize>) -> bool {
        // Never write a semicolon after a comment; it would land outside the
        // statement it is meant to terminate.
        if self
            .prev_emitted
            .is_some_and(|index| self.tokens[index].kind.is_comment())
        {
            return false;
        }
        let Some(prev) = self.prev_significant else {
            return false;
        };
        let prev_kind = self.tokens[prev].kind;
        let ends_statement = ends_a_statement(prev_kind)
            || (prev_kind == TokenKind::Punctuator(Punctuator::CloseBrace)
                && self.analysis.annos[prev].object_close);
        if !ends_statement || self.analysis.annos[prev].in_jsx {
            return false;
        }
        if !self.in_statement_position() {
            return false;
        }
        // A comment sitting between two statements must not hide the statement
        // that follows it.
        let next = next.and_then(|index| self.next_program_token(index));
        match next {
            None => true,
            Some(index) => starts_a_statement(self.tokens[index].kind),
        }
    }

    /// The token at `index`, or the first non-comment token after it.
    fn next_program_token(&self, index: usize) -> Option<usize> {
        self.tokens[index..]
            .iter()
            .position(|token| !token.kind.is_trivia())
            .map(|offset| index + offset)
    }

    /// Whether the innermost open group holds statements: the file itself, a
    /// block, a class body or a switch body. Parenthesised groups have no
    /// statements and object literals separate their members with commas.
    fn in_statement_position(&self) -> bool {
        self.groups.last().is_none_or(|group| group.statements)
    }

    /// Whether a `;` may be removed when semicolons are switched off.
    fn can_drop_semicolon(&self, index: usize) -> bool {
        let Some(prev) = self.prev_significant else {
            return false;
        };
        let prev_kind = self.tokens[prev].kind;
        // A `}` cannot gain a synthesized semicolon, because a block must not get
        // one, but an existing `};` after an object or class may be dropped.
        if !ends_a_statement(prev_kind)
            && prev_kind != TokenKind::Punctuator(Punctuator::CloseBrace)
        {
            return false;
        }
        // `if (x);` is an empty statement: dropping the `;` would adopt the next
        // statement as the body.
        if prev_kind == TokenKind::Punctuator(Punctuator::CloseParen)
            && self.analysis.annos[prev].statement_paren
        {
            return false;
        }
        if !self.in_statement_position() {
            return false;
        }
        match self.next_significant(index) {
            None => true,
            Some(next) => starts_a_statement(self.tokens[next].kind),
        }
    }
}

/// Whether a statement may end with this token.
fn ends_a_statement(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier
            | TokenKind::PrivateName
            | TokenKind::Number
            | TokenKind::String
            | TokenKind::Regex
            | TokenKind::TemplateFull
            | TokenKind::TemplateTail
            | TokenKind::JsxTagEnd
            | TokenKind::JsxSelfClose
            | TokenKind::Punctuator(
                Punctuator::CloseParen
                    | Punctuator::CloseBracket
                    | Punctuator::PlusPlus
                    | Punctuator::MinusMinus
            )
            | TokenKind::Keyword(
                Keyword::This
                    | Keyword::Super
                    | Keyword::Null
                    | Keyword::True
                    | Keyword::False
                    | Keyword::Break
                    | Keyword::Continue
                    | Keyword::Return
                    | Keyword::Debugger
            )
    )
}

/// Whether a new statement may begin with this token.
///
/// Everything that could instead continue the previous expression — `(`, `[`, a
/// template literal, an operator, `in`/`of`/`instanceof` — is excluded, which is
/// exactly the set of tokens for which automatic semicolon insertion does not
/// fire.
fn starts_a_statement(kind: TokenKind) -> bool {
    match kind {
        TokenKind::Identifier | TokenKind::PrivateName => true,
        TokenKind::Keyword(keyword) => !matches!(
            keyword,
            Keyword::In
                | Keyword::Of
                | Keyword::Instanceof
                | Keyword::Extends
                | Keyword::As
                | Keyword::From
                | Keyword::Implements
                | Keyword::Mixins
                | Keyword::Renders
        ),
        TokenKind::Punctuator(punctuator) => matches!(
            punctuator,
            Punctuator::OpenBrace | Punctuator::CloseBrace | Punctuator::At
        ),
        _ => false,
    }
}

// ------------------------------------------------------------ quote handling

/// Rewrite a string literal into the configured quote style, but only when that
/// does not require more escaping than the original spelling.
fn requote(text: &str, style: QuoteStyle) -> Option<String> {
    let bytes = text.as_bytes();
    if bytes.len() < 2 {
        return None;
    }
    let quote = bytes[0];
    if (quote != b'\'' && quote != b'"') || bytes[bytes.len() - 1] != quote {
        return None;
    }

    let body = &text[1..text.len() - 1];
    let (mut doubles, mut singles) = (0usize, 0usize);
    let mut chars = body.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => match chars.next() {
                Some('"') => doubles += 1,
                Some('\'') => singles += 1,
                _ => {}
            },
            '"' => doubles += 1,
            '\'' => singles += 1,
            _ => {}
        }
    }

    let chosen = match style {
        QuoteStyle::Double if doubles > singles => '\'',
        QuoteStyle::Double => '"',
        QuoteStyle::Single if singles > doubles => '"',
        QuoteStyle::Single => '\'',
    };
    if chosen as u8 == quote {
        return None;
    }

    let mut rewritten = String::with_capacity(text.len() + 4);
    rewritten.push(chosen);
    let mut chars = body.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                None => rewritten.push('\\'),
                Some(next) => {
                    // An escaped quote that is no longer the delimiter can drop
                    // its backslash; every other escape is copied verbatim.
                    if (next == '"' || next == '\'') && next != chosen {
                        rewritten.push(next);
                    } else {
                        rewritten.push('\\');
                        rewritten.push(next);
                    }
                }
            }
        } else if ch == chosen {
            rewritten.push('\\');
            rewritten.push(chosen);
        } else {
            rewritten.push(ch);
        }
    }
    rewritten.push(chosen);
    Some(rewritten)
}

/// JSX attribute strings have no escape sequences, so they may only be requoted
/// when the body does not contain the target quote at all.
fn requote_jsx(text: &str, style: QuoteStyle) -> Option<String> {
    let bytes = text.as_bytes();
    if bytes.len() < 2 {
        return None;
    }
    let quote = bytes[0];
    if (quote != b'\'' && quote != b'"') || bytes[bytes.len() - 1] != quote {
        return None;
    }
    let target = match style {
        QuoteStyle::Double => b'"',
        QuoteStyle::Single => b'\'',
    };
    if target == quote {
        return None;
    }
    let body = &text[1..text.len() - 1];
    if body.as_bytes().contains(&target) {
        return None;
    }
    let mut rewritten = String::with_capacity(text.len());
    rewritten.push(char::from(target));
    rewritten.push_str(body);
    rewritten.push(char::from(target));
    Some(rewritten)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_quotes_become_double_quotes() {
        assert_eq!(requote("'a'", QuoteStyle::Double).as_deref(), Some("\"a\""));
    }

    #[test]
    fn already_preferred_quotes_are_left_alone() {
        assert_eq!(requote("\"a\"", QuoteStyle::Double), None);
        assert_eq!(requote("'a'", QuoteStyle::Single), None);
    }

    #[test]
    fn a_string_full_of_double_quotes_keeps_single_quotes() {
        assert_eq!(requote("'say \"hi\"'", QuoteStyle::Double), None);
    }

    #[test]
    fn converting_drops_now_redundant_escapes() {
        assert_eq!(
            requote("'it\\'s'", QuoteStyle::Double).as_deref(),
            Some("\"it's\"")
        );
    }

    #[test]
    fn converting_adds_escapes_only_when_it_does_not_lose_ground() {
        // One of each: the preferred quote wins and the escape count is unchanged.
        assert_eq!(
            requote("'a\"b\\'c'", QuoteStyle::Double).as_deref(),
            Some("\"a\\\"b'c\"")
        );
    }

    #[test]
    fn other_escapes_survive_requoting() {
        assert_eq!(
            requote("'a\\nb\\u0041\\\\'", QuoteStyle::Double).as_deref(),
            Some("\"a\\nb\\u0041\\\\\"")
        );
    }

    #[test]
    fn line_continuations_survive_requoting() {
        assert_eq!(
            requote("'a\\\nb'", QuoteStyle::Double).as_deref(),
            Some("\"a\\\nb\"")
        );
    }

    #[test]
    fn requoting_ignores_malformed_literals() {
        assert_eq!(requote("'", QuoteStyle::Double), None);
        assert_eq!(requote("'abc", QuoteStyle::Double), None);
        assert_eq!(requote("", QuoteStyle::Double), None);
    }

    #[test]
    fn jsx_strings_are_requoted_only_when_no_escape_would_be_needed() {
        assert_eq!(
            requote_jsx("'a'", QuoteStyle::Double).as_deref(),
            Some("\"a\"")
        );
        assert_eq!(requote_jsx("'a\"b'", QuoteStyle::Double), None);
    }

    #[test]
    fn display_width_counts_characters_after_the_last_newline() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("ab\ncde"), 3);
        assert_eq!(display_width("日本"), 2);
    }
}
