//! The `curl … | sh` installer, checked against the binary it installs.
//!
//! `infra/cloudflare/setup-assets/install.sh` draws the same banner `uf` draws,
//! and has to decide the same way whether the terminal can take a picture. It
//! cannot call into `uf_term` — it runs before any of uf exists on the machine
//! — so the decision is written twice, and these tests are what keeps the two
//! copies the same. A terminal the binary draws a logo on and the installer
//! draws blocks on is not a disaster, but it is the kind of drift nobody
//! notices until someone asks why the installer looks worse than the tool.

use std::fs;
use std::path::{Path, PathBuf};

use uf_term::{ImageEnv, ImageProtocol};

fn installer() -> String {
    let path = repository_root().join("infra/cloudflare/setup-assets/install.sh");
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the crate lives inside the repository")
}

/// Every terminal the binary recognises, and the protocol it picks for it.
///
/// The table is written here rather than derived, so a terminal added to one
/// side and not the other is a failure rather than a silently shorter list.
const TERMINALS: &[(&str, ImageProtocol)] = &[
    ("kitty", ImageProtocol::Kitty),
    ("ghostty", ImageProtocol::Kitty),
    ("WezTerm", ImageProtocol::Kitty),
    ("iTerm.app", ImageProtocol::ITerm2),
    ("vscode", ImageProtocol::ITerm2),
    ("Hyper", ImageProtocol::ITerm2),
    ("rio", ImageProtocol::ITerm2),
];

#[test]
fn the_installer_knows_every_terminal_the_binary_knows() {
    let script = installer();

    for (terminal, _) in TERMINALS {
        assert!(
            script.contains(terminal),
            "the installer does not mention {terminal}, which `uf` draws a logo on"
        );
    }
    assert!(
        script.contains("KITTY_WINDOW_ID") && script.contains("KONSOLE_VERSION"),
        "the installer must check the same two identifying variables the binary does"
    );
}

/// The table above has to be the binary's actual behaviour, not a wish.
#[test]
fn the_binary_picks_the_protocol_the_table_claims() {
    for (terminal, protocol) in TERMINALS {
        let by_term_program = ImageEnv::default().with_term_program(terminal).protocol();
        let by_term = ImageEnv::default()
            .with_term(&format!("xterm-{}", terminal.to_lowercase()))
            .protocol();

        assert!(
            by_term_program == Some(*protocol) || by_term == Some(*protocol),
            "{terminal} should resolve to {protocol:?}, got {by_term_program:?} by TERM_PROGRAM \
             and {by_term:?} by TERM"
        );
    }
}

#[test]
fn the_installer_refuses_inline_images_inside_a_multiplexer() {
    let script = installer();

    assert!(
        script.contains("TMUX") && script.contains("STY"),
        "a multiplexer rewrites the stream, and the installer must refuse for the same \
         reason the binary does"
    );
}

#[test]
fn the_installer_honours_the_same_override_variable() {
    let script = installer();

    assert!(
        script.contains("UF_INLINE_IMAGES"),
        "the one escape hatch has to work in both places or it is not an escape hatch"
    );
}

/// The kitty protocol discards a sequence whose payload is over the limit, so
/// an installer that stopped chunking would silently print nothing.
#[test]
fn the_installer_chunks_its_kitty_payload() {
    let script = installer();

    assert!(
        script.contains("fold -w 4096"),
        "the embedded logo is larger than one kitty sequence may carry"
    );
    assert!(
        script.contains("m=1") && script.contains("m=0"),
        "chunks must say whether more is coming"
    );
}

/// The `size` key iTerm2 reads has to be the real decoded length.
#[test]
fn the_installers_declared_logo_size_is_the_logo_it_embeds() {
    let script = installer();

    let declared: usize = script
        .lines()
        .find_map(|line| line.strip_prefix("uf_logo_bytes="))
        .expect("the installer states the logo's decoded size")
        .trim()
        .parse()
        .expect("the size is a number");

    let payload = script
        .split_once("<<'UF_LOGO_PNG'\n")
        .expect("the logo is embedded in a heredoc")
        .1
        .split_once("\nUF_LOGO_PNG")
        .expect("the heredoc is terminated")
        .0
        .replace('\n', "");

    // Base64 without padding carries 6 bits per character; the padding says how
    // many of the final three bytes are real.
    let padding = payload.chars().rev().take_while(|&c| c == '=').count();
    let decoded = payload.len() / 4 * 3 - padding;

    assert_eq!(
        decoded, declared,
        "the installer tells iTerm2 the image is {declared} bytes, but embeds {decoded}"
    );
}
