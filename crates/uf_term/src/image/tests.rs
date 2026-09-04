use super::*;

/// A payload long enough to force kitty's chunking, and short enough to read.
fn long_png() -> Vec<u8> {
    // 4096 base64 characters carry 3072 bytes, so this is just over two chunks.
    (0..7000u32).map(|byte| byte as u8).collect()
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n-not-really-a-png";
const PLACEMENT: Placement = Placement::new(20, 5);

#[test]
fn an_empty_payload_produces_no_escape_at_all() {
    for protocol in [ImageProtocol::Kitty, ImageProtocol::ITerm2] {
        assert_eq!(
            inline_image(&[], protocol, PLACEMENT),
            None,
            "{protocol:?} should refuse an empty image"
        );
    }
}

#[test]
fn the_kitty_escape_transmits_a_png_and_displays_it_in_cells() {
    let escape = inline_image(PNG, ImageProtocol::Kitty, PLACEMENT).expect("a payload");

    assert!(escape.starts_with("\x1b_Ga=T,f=100,t=d,c=20,r=5,m=0;"));
    assert!(escape.ends_with("\x1b\\"));
}

#[test]
fn a_long_kitty_payload_is_chunked_and_only_the_last_chunk_says_so() {
    let png = long_png();
    let escape = inline_image(&png, ImageProtocol::Kitty, PLACEMENT).expect("a payload");

    let chunks = escape.split("\x1b_G").skip(1).collect::<Vec<_>>();
    assert!(chunks.len() > 1, "7000 bytes must not fit in one chunk");

    for (index, chunk) in chunks.iter().enumerate() {
        let last = index + 1 == chunks.len();
        let payload = chunk.split_once(';').expect("keys then payload").1;
        let payload = payload.strip_suffix("\x1b\\").expect("chunk terminator");

        assert!(
            payload.len() <= MAX_PAYLOAD_CHARS,
            "chunk {index} carries {} characters, over the protocol's limit",
            payload.len()
        );
        assert_eq!(
            chunk.contains("m=0;"),
            last,
            "only the last chunk may say the transmission is complete"
        );
        assert_eq!(
            chunk.contains("a=T"),
            index == 0,
            "only the first chunk carries the keys"
        );
    }
}

/// Reassembling every chunk's payload must give back the original image.
#[test]
fn the_chunks_of_a_kitty_escape_reassemble_into_the_whole_payload() {
    let png = long_png();
    let escape = inline_image(&png, ImageProtocol::Kitty, PLACEMENT).expect("a payload");

    let reassembled = escape
        .split("\x1b_G")
        .skip(1)
        .map(|chunk| {
            chunk
                .split_once(';')
                .expect("keys then payload")
                .1
                .strip_suffix("\x1b\\")
                .expect("chunk terminator")
        })
        .collect::<String>();

    let mut expected = String::new();
    base64::encode_into(&png, &mut expected);
    assert_eq!(reassembled, expected);
}

#[test]
fn the_iterm2_escape_is_one_sequence_carrying_the_decoded_size() {
    let escape = inline_image(PNG, ImageProtocol::ITerm2, PLACEMENT).expect("a payload");

    assert!(escape.starts_with("\x1b]1337;File=inline=1;preserveAspectRatio=1;size="));
    assert!(escape.contains(&format!("size={};", PNG.len())));
    assert!(escape.contains("width=20;height=5:"));
    assert!(escape.ends_with('\x07'));
    assert_eq!(escape.matches("\x1b]1337").count(), 1);
}

#[test]
fn the_iterm2_payload_is_the_base64_of_the_image() {
    let escape = inline_image(PNG, ImageProtocol::ITerm2, PLACEMENT).expect("a payload");

    let payload = escape
        .rsplit_once(':')
        .expect("keys then payload")
        .1
        .strip_suffix('\x07')
        .expect("bell terminator");

    let mut expected = String::new();
    base64::encode_into(PNG, &mut expected);
    assert_eq!(payload, expected);
}

#[test]
fn a_placement_is_reported_in_the_escape_it_produces() {
    let escape =
        inline_image(PNG, ImageProtocol::Kitty, Placement::new(120, 40)).expect("a payload");
    assert!(escape.contains("c=120,r=40"), "{escape:?}");

    let escape =
        inline_image(PNG, ImageProtocol::ITerm2, Placement::new(120, 40)).expect("a payload");
    assert!(escape.contains("width=120;height=40"), "{escape:?}");
}

#[test]
fn numbers_are_written_without_allocating_a_string_per_number() {
    let mut out = String::new();
    for value in [0u32, 1, 9, 10, 99, 100, 4096, 65535, 1_000_000] {
        out.clear();
        push_number(&mut out, value);
        assert_eq!(out, value.to_string(), "for {value}");
    }
}

// --- protocol detection ------------------------------------------------

#[test]
fn an_unrecognised_terminal_gets_no_protocol() {
    assert_eq!(ImageEnv::default().protocol(), None);
    assert_eq!(
        ImageEnv::default().with_term("xterm-256color").protocol(),
        None
    );
    assert_eq!(ImageEnv::default().with_term("dumb").protocol(), None);
}

#[test]
fn kitty_is_recognised_by_its_window_id_and_by_terminfo() {
    assert_eq!(
        ImageEnv::default().with_kitty_window_id("1").protocol(),
        Some(ImageProtocol::Kitty)
    );
    assert_eq!(
        ImageEnv::default().with_term("xterm-kitty").protocol(),
        Some(ImageProtocol::Kitty)
    );
}

#[test]
fn ghostty_and_wezterm_speak_the_kitty_protocol() {
    assert_eq!(
        ImageEnv::default().with_term("xterm-ghostty").protocol(),
        Some(ImageProtocol::Kitty)
    );
    assert_eq!(
        ImageEnv::default().with_term_program("WezTerm").protocol(),
        Some(ImageProtocol::Kitty)
    );
}

#[test]
fn iterm2_konsole_and_vscode_speak_the_iterm2_protocol() {
    assert_eq!(
        ImageEnv::default()
            .with_term_program("iTerm.app")
            .protocol(),
        Some(ImageProtocol::ITerm2)
    );
    assert_eq!(
        ImageEnv::default().with_term_program("vscode").protocol(),
        Some(ImageProtocol::ITerm2)
    );
    assert_eq!(
        ImageEnv::default()
            .with_konsole_version("220803")
            .protocol(),
        Some(ImageProtocol::ITerm2)
    );
}

/// A multiplexer rewrites the stream, and the failure mode is escape-sequence
/// garbage across the pane rather than a missing picture.
#[test]
fn a_multiplexer_turns_inline_images_off() {
    assert_eq!(
        ImageEnv::default()
            .with_kitty_window_id("1")
            .with_tmux("/tmp/tmux-501/default,1,0")
            .protocol(),
        None
    );
    assert_eq!(
        ImageEnv::default()
            .with_term_program("iTerm.app")
            .with_sty("4242.pts-0.host")
            .protocol(),
        None
    );
}

#[test]
fn the_environment_override_wins_in_both_directions() {
    assert_eq!(
        ImageEnv::default()
            .with_kitty_window_id("1")
            .with_uf_inline_images("0")
            .protocol(),
        None,
        "a recognised terminal can be told to stop"
    );
    assert_eq!(
        ImageEnv::default()
            .with_uf_inline_images("kitty")
            .protocol(),
        Some(ImageProtocol::Kitty),
        "an unrecognised terminal can be told to try"
    );
    assert_eq!(
        ImageEnv::default()
            .with_uf_inline_images("iterm2")
            .protocol(),
        Some(ImageProtocol::ITerm2)
    );
}

/// `UF_INLINE_IMAGES=1` asks for the best available rather than forcing one,
/// so it must not override the multiplexer refusal or invent a protocol.
#[test]
fn asking_for_images_in_general_falls_through_to_detection() {
    assert_eq!(
        ImageEnv::default().with_uf_inline_images("1").protocol(),
        None
    );
    assert_eq!(
        ImageEnv::default()
            .with_uf_inline_images("1")
            .with_term("xterm-kitty")
            .protocol(),
        Some(ImageProtocol::Kitty)
    );
}

#[test]
fn an_empty_variable_is_the_same_as_an_unset_one() {
    assert_eq!(
        ImageEnv::default()
            .with_term("xterm-kitty")
            .with_tmux("")
            .protocol(),
        Some(ImageProtocol::Kitty),
        "an empty TMUX is not a tmux session"
    );
    assert_eq!(
        ImageEnv::default().with_kitty_window_id("").protocol(),
        None
    );
}

// --- fitting a box to an image ------------------------------------------

/// A square image needs a box twice as wide in cells as it is tall, because a
/// cell is about twice as tall as it is wide.
#[test]
fn a_square_image_gets_a_square_box() {
    assert_eq!(Placement::fitting(512, 512, 5), Placement::new(10, 5));
    assert_eq!(Placement::fitting(128, 128, 4), Placement::new(8, 4));
}

#[test]
fn a_wide_image_gets_a_wide_box() {
    // 713x195 is roughly 3.66:1, so five rows want about 37 columns.
    assert_eq!(Placement::fitting(713, 195, 5), Placement::new(37, 5));
}

#[test]
fn a_tall_image_gets_a_narrow_box() {
    assert_eq!(Placement::fitting(100, 400, 8), Placement::new(4, 8));
}

/// Never zero columns: a box no cells wide displays nothing at all.
#[test]
fn a_very_tall_image_still_gets_a_column() {
    assert_eq!(Placement::fitting(1, 10_000, 1), Placement::new(1, 1));
}

#[test]
fn a_degenerate_image_gets_a_square_box_rather_than_dividing_by_zero() {
    assert_eq!(Placement::fitting(0, 100, 5), Placement::new(5, 5));
    assert_eq!(Placement::fitting(100, 0, 5), Placement::new(5, 5));
}
