//! Parsing one file into everything the checker needs from it.
//!
//! The docblock is deliberately consumed here rather than stored. Upstream's
//! `Docblock` type is private to `flow_parsing`, so the only thing an embedder
//! can do with one is fold it into a [`Metadata`] immediately — which is also
//! the only thing anyone wants it for.

use std::sync::Arc;

use dupe::Dupe;
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
}

impl Parsed {
    /// Whether the file parsed cleanly enough to type check.
    pub(super) fn is_parseable(&self) -> bool {
        self.parse_errors.is_empty()
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
    }
}
