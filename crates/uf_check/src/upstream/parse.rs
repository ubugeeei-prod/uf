//! Parsing one file into everything the checker needs from it.
//!
//! The docblock is folded into [`Metadata`] here rather than stored, because
//! that is the only shape the checker takes it in. One bit is read out first:
//! whether the file opted out of Flow entirely, which `Metadata` cannot carry
//! — upstream sets `checked` from `@flow` but deliberately never clears it for
//! `@noflow`, on the grounds that `--all` outranks the opt-out. uf runs with
//! `all` on so that a file with no pragma is still checked, and then honours
//! `@noflow` on top, so the two halves of that policy are decided here.

use std::sync::Arc;

use dupe::Dupe;
use flow_common::docblock::FlowMode;
use flow_common::options::Options;
use flow_parser::PERMISSIVE_PARSE_OPTIONS;
use flow_parser::ast;
use flow_parser::file_key::FileKey;
use flow_parser::loc::Loc;
use flow_parser::parse_error::ParseError;
use flow_parser_utils::file_sig::FileSig;
use flow_parsing::docblock_parser;
use flow_typing_context::Metadata;

use super::options::file_sig_options;

/// One parsed file.
pub(super) struct Parsed {
    /// The key Flow identifies the file by.
    pub(super) file_key: FileKey,
    /// Checker metadata, with this file's docblock already applied.
    pub(super) metadata: Metadata,
    /// The untyped AST.
    pub(super) ast: Arc<ast::Program<Loc, Loc>>,
    /// The module's imports and exports.
    pub(super) file_sig: Arc<FileSig>,
    /// Syntax errors, in source order. Non-empty means inference must not run.
    pub(super) parse_errors: Vec<(Loc, ParseError)>,
    /// Whether the file's docblock says `@noflow`.
    opted_out: bool,
}

impl Parsed {
    /// Whether the file parsed cleanly enough to type check.
    pub(super) fn is_parseable(&self) -> bool {
        self.parse_errors.is_empty()
    }

    /// Whether inference should run over this file.
    ///
    /// `@noflow` is Flow's own way for a file to say it is plain JavaScript,
    /// and it is the only one uf offers: a config key listing paths would be a
    /// second, uf-shaped answer to a question Flow has already answered, and
    /// the file itself is where a reader looks. Parse errors are reported
    /// either way — uf transforms every `.js` it owns regardless of docblock,
    /// so a file that does not parse is broken whatever it opted out of.
    pub(super) fn is_checked(&self) -> bool {
        !self.opted_out
    }
}

/// Parse `content` as `file_key`.
///
/// `is_lib_file` changes how the module signature is built, not how the source
/// is parsed: library definitions may declare globals.
pub(super) fn parse_file(
    file_key: FileKey,
    content: &str,
    options: &Options,
    is_lib_file: bool,
) -> Parsed {
    let (_docblock_errors, docblock) = docblock_parser::parse_docblock(
        options.max_header_tokens as usize,
        &options.file_options,
        &file_key,
        content,
    );
    let metadata = flow_typing_context::docblock_overrides(
        &docblock,
        &file_key,
        flow_typing_context::mk_context_metadata(options, Arc::default()),
    );
    let (ast, parse_errors) = flow_parser::parse_program_file::<()>(
        false,
        None,
        Some(PERMISSIVE_PARSE_OPTIONS),
        file_key.dupe(),
        Ok(content),
    );
    let file_sig = Arc::new(FileSig::from_program(
        &file_key,
        &ast,
        &file_sig_options(options, is_lib_file),
    ));

    Parsed {
        file_key,
        metadata,
        ast: Arc::new(ast),
        file_sig,
        parse_errors,
        opted_out: matches!(docblock.flow(), Some(FlowMode::OptOut)),
    }
}
