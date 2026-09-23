//! The editor integrations that are files, compiled into `uf`.
//!
//! Everything under `editors/` except the VS Code extension is configuration
//! or source a user copies: a Lua module, a Vim script, an Emacs Lisp file, an
//! LSP4IJ template, the Zed extension's sources, a Helix `languages.toml`.
//! `uf editor install` and `uf editor setup` write them from here rather than
//! downloading them, for the reason `uf ui add` carries its components: the
//! copy written is the one that matches the `uf` writing it, and writing it
//! needs no network. The VS Code extension is the exception because it is a
//! packaged `.vsix` with its `node_modules`, built by CI; see `install.rs`.
//!
//! The paths are relative to this file, so a file moved under `editors/` is a
//! compile error here rather than a stale copy.

/// One file to write: where it goes, relative to the directory it is
/// installed into, and what it says.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Asset {
    pub(crate) path: &'static str,
    pub(crate) contents: &'static str,
}

/// `editors/neovim/lua/uf.lua`: `require("uf").setup()`.
pub(crate) const NEOVIM: &str = include_str!("../../../../../editors/neovim/lua/uf.lua");

/// `editors/vim/uf.vim`: a vim-lsp registration.
pub(crate) const VIM: &str = include_str!("../../../../../editors/vim/uf.vim");

/// `editors/emacs/uf.el`: Eglot and lsp-mode.
pub(crate) const EMACS: &str = include_str!("../../../../../editors/emacs/uf.el");

/// `editors/helix/languages.toml`, which `uf editor setup helix` writes as a
/// project's `.helix/languages.toml`.
pub(crate) const HELIX: &str = include_str!("../../../../../editors/helix/languages.toml");

/// `editors/jetbrains/lsp4ij-template`, the directory LSP4IJ imports.
pub(crate) const JETBRAINS: &[Asset] = &[Asset {
    path: "template.json",
    contents: include_str!("../../../../../editors/jetbrains/lsp4ij-template/template.json"),
}];

/// `editors/zed`: the sources Zed compiles when it installs a dev extension.
///
/// Sources rather than a built `.wasm`: Zed has no command-line install and
/// no supported way to side-load a compiled extension, and its "install dev
/// extension" action compiles what it is pointed at with the toolchain it
/// manages. So the honest install is to put these where the user can point
/// Zed at them.
pub(crate) const ZED: &[Asset] = &[
    Asset {
        path: "extension.toml",
        contents: include_str!("../../../../../editors/zed/extension.toml"),
    },
    Asset {
        path: "Cargo.toml",
        contents: include_str!("../../../../../editors/zed/Cargo.toml"),
    },
    Asset {
        path: "Cargo.lock",
        contents: include_str!("../../../../../editors/zed/Cargo.lock"),
    },
    Asset {
        path: "src/lib.rs",
        contents: include_str!("../../../../../editors/zed/src/lib.rs"),
    },
    Asset {
        path: "src/command.rs",
        contents: include_str!("../../../../../editors/zed/src/command.rs"),
    },
    Asset {
        path: "README.md",
        contents: include_str!("../../../../../editors/zed/README.md"),
    },
];
