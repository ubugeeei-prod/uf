use camino::Utf8PathBuf;

use super::*;

/// The three tables this crate reads, filled in with the numbers a test wants.
///
/// Built rather than checked in: a fixture font would be a binary blob nobody
/// can read a diff of, and every field that matters here is one of six
/// numbers.
fn tables(units_per_em: u16, ascent: i16, descent: i16, line_gap: i16, average: i16) -> Tables {
    let mut head = vec![0u8; 54];
    head[18..20].copy_from_slice(&units_per_em.to_be_bytes());

    let mut hhea = vec![0u8; 36];
    hhea[4..6].copy_from_slice(&ascent.to_be_bytes());
    hhea[6..8].copy_from_slice(&descent.to_be_bytes());
    hhea[8..10].copy_from_slice(&line_gap.to_be_bytes());

    // Version 4, which is long enough to carry every field this crate reads.
    let mut os2 = vec![0u8; 96];
    os2[0..2].copy_from_slice(&4u16.to_be_bytes());
    os2[2..4].copy_from_slice(&average.to_be_bytes());

    vec![(*b"head", head), (*b"hhea", hhea), (*b"OS/2", os2)]
}

/// Pack tables into a bare SFNT.
fn sfnt(tables: &Tables) -> Vec<u8> {
    let count = tables.len();
    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&(count as u16).to_be_bytes());
    // searchRange, entrySelector, rangeShift: no reader here uses them.
    out.extend_from_slice(&[0u8; 6]);
    let mut offset = 12 + count * 16;
    for (tag, data) in tables {
        out.extend_from_slice(tag);
        out.extend_from_slice(&0u32.to_be_bytes()); // checksum
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        offset += data.len();
    }
    for (_, data) in tables {
        out.extend_from_slice(data);
    }
    out
}

/// Pack tables into a WOFF2, brotli and all.
fn woff2(tables: &Tables) -> Vec<u8> {
    let index_of = |tag: &[u8; 4]| {
        WOFF2_TAGS
            .iter()
            .position(|known| *known == tag)
            .expect("the test only uses known tags") as u8
    };

    let mut stream = Vec::new();
    for (_, data) in tables {
        stream.extend_from_slice(data);
    }
    let mut compressed = Vec::new();
    {
        use std::io::Write as _;
        let mut writer = brotli::CompressorWriter::new(&mut compressed, 4096, 11, 22);
        writer.write_all(&stream).unwrap();
    }

    let mut directory = Vec::new();
    for (tag, data) in tables {
        directory.push(index_of(tag));
        directory.extend_from_slice(&base128(data.len() as u32));
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"wOF2");
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // flavor
    out.extend_from_slice(&0u32.to_be_bytes()); // length, unread here
    out.extend_from_slice(&(tables.len() as u16).to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // reserved
    out.extend_from_slice(&(stream.len() as u32).to_be_bytes()); // totalSfntSize
    out.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
    out.extend_from_slice(&[0u8; 4]); // major/minor version
    out.extend_from_slice(&[0u8; 20]); // meta and private block offsets
    debug_assert_eq!(out.len(), 48);
    out.extend_from_slice(&directory);
    out.extend_from_slice(&compressed);
    out
}

/// WOFF2's `UIntBase128`, for building a fixture.
fn base128(mut value: u32) -> Vec<u8> {
    let mut digits = Vec::new();
    loop {
        digits.push((value & 0x7f) as u8);
        value >>= 7;
        if value == 0 {
            break;
        }
    }
    digits.reverse();
    let last = digits.len() - 1;
    for (index, digit) in digits.iter_mut().enumerate() {
        if index != last {
            *digit |= 0x80;
        }
    }
    digits
}

fn temp() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    (dir, path)
}

fn request<'a>(
    source: &'a Utf8Path,
    out: &'a Utf8Path,
    fallback: Option<&'a str>,
) -> FontRequest<'a> {
    FontRequest {
        source,
        family: "Test Sans",
        weight: "400",
        style: "normal",
        display: "swap",
        fallback,
        out_dir: out,
        base_url: "/assets/",
        subset: crate::subset::SubsetMode::Off,
        preload: true,
    }
}

#[test]
fn the_metrics_come_out_of_a_bare_sfnt() {
    let bytes = sfnt(&tables(1000, 800, -200, 0, 500));
    let (container, metrics) = read_metrics(Utf8Path::new("t.ttf"), &bytes).unwrap();

    assert_eq!(container, FontContainer::Sfnt);
    assert_eq!(metrics.units_per_em, 1000);
    assert_eq!(metrics.ascent, 800);
    assert_eq!(metrics.descent, -200);
    assert_eq!(metrics.average_width, Some(500));
}

#[test]
fn the_metrics_come_out_of_a_woff2_the_same_way() {
    // The container a project actually ships. Its tables are inside one brotli
    // stream, so reading them at all means decompressing and walking a
    // directory whose lengths are base-128 — none of which the caller should
    // have to care about.
    let source = tables(2048, 1900, -500, 0, 1100);
    let (from_sfnt, _) = (
        read_metrics(Utf8Path::new("t.ttf"), &sfnt(&source)).unwrap(),
        (),
    );
    let (container, from_woff2) = read_metrics(Utf8Path::new("t.woff2"), &woff2(&source)).unwrap();

    assert_eq!(container, FontContainer::Woff2);
    assert_eq!(from_woff2, from_sfnt.1);
}

#[test]
fn a_font_with_an_empty_hhea_falls_back_to_the_os2_typo_metrics() {
    // Some CFF fonts leave `hhea` at zero. A zero line box would make every
    // override zero and the fallback face a flat line.
    let mut built = tables(1000, 0, 0, 0, 500);
    let os2 = &mut built[2].1;
    os2[68..70].copy_from_slice(&750i16.to_be_bytes());
    os2[70..72].copy_from_slice(&(-250i16).to_be_bytes());
    os2[72..74].copy_from_slice(&0i16.to_be_bytes());

    let (_, metrics) = read_metrics(Utf8Path::new("t.otf"), &sfnt(&built)).unwrap();

    assert_eq!(metrics.ascent, 750);
    assert_eq!(metrics.descent, -250);
}

#[test]
fn the_overrides_scale_the_fallback_to_the_real_face() {
    // The arithmetic, on numbers a reader can check by hand. The real face is
    // twice as wide per em as Arial, so the fallback has to be scaled to 200%
    // and each vertical metric divided by that same factor.
    let real = FontMetrics {
        units_per_em: 1000,
        ascent: 800,
        descent: -200,
        line_gap: 0,
        average_width: Some(1000),
    };
    let arial = local_face("Arial").unwrap();
    // Arial is 904/2048 = 0.44140625 em wide on average; the face above is
    // 1000/1000 = 1.0, so size-adjust is 1.0 / 0.44140625 = 226.55%.
    let matched = real.fallback_for(arial).unwrap();

    assert_eq!(matched.local, "Arial");
    assert_eq!(matched.size_adjust, "226.55%");
    // 800/1000 / 2.2655 = 35.31%
    assert_eq!(matched.ascent_override, "35.31%");
    assert_eq!(matched.descent_override, "8.83%");
    assert_eq!(matched.line_gap_override, "0%");
}

#[test]
fn a_font_with_no_average_width_gets_no_size_adjust_rather_than_a_guessed_one() {
    // A fallback carrying three overrides and no `size-adjust` matches the
    // line box and gets the line *length* wrong, which moves text sideways.
    // No answer is the right answer.
    let real = FontMetrics {
        units_per_em: 1000,
        ascent: 800,
        descent: -200,
        line_gap: 0,
        average_width: None,
    };

    assert!(real.fallback_for(local_face("Arial").unwrap()).is_none());
}

#[test]
fn the_font_is_self_hosted_under_a_content_hashed_name() {
    let (_guard, dir) = temp();
    let source = dir.join("TestSans.woff2");
    std::fs::write(&source, woff2(&tables(1000, 800, -200, 0, 500))).unwrap();
    let out = dir.join("out");

    let asset = self_host(&request(&source, &out, Some("Arial"))).unwrap();

    let written = out.join(&asset.file);
    assert!(written.exists(), "{written} was not written");
    assert_eq!(
        std::fs::read(&written).unwrap(),
        std::fs::read(&source).unwrap()
    );
    assert_eq!(asset.bytes, std::fs::metadata(&written).unwrap().len());
    assert_eq!(asset.mime, "font/woff2");
    assert_ne!(
        asset.file, "TestSans.woff2",
        "the name has to carry a digest"
    );
    assert!(asset.file.starts_with("TestSans."), "{}", asset.file);
}

#[test]
fn the_stylesheet_declares_the_real_face_and_the_matched_fallback() {
    let (_guard, dir) = temp();
    let source = dir.join("TestSans.woff2");
    std::fs::write(&source, woff2(&tables(1000, 800, -200, 0, 500))).unwrap();
    let out = dir.join("out");

    let asset = self_host(&request(&source, &out, Some("Arial"))).unwrap();

    assert!(
        asset.css.contains(r#"font-family:"Test Sans""#),
        "{}",
        asset.css
    );
    assert!(asset.css.contains("font-display:swap"), "{}", asset.css);
    assert!(
        asset
            .css
            .contains(&format!(r#"url("/assets/{}")"#, asset.file)),
        "{}",
        asset.css
    );
    assert!(asset.css.contains("format(\"woff2\")"), "{}", asset.css);

    // The second face: no download, a local source, and the four numbers.
    assert!(
        asset.css.contains(r#"font-family:"Test Sans Fallback""#),
        "{}",
        asset.css
    );
    assert!(asset.css.contains(r#"src:local("Arial")"#), "{}", asset.css);
    for property in [
        "size-adjust:",
        "ascent-override:",
        "descent-override:",
        "line-gap-override:",
    ] {
        assert!(
            asset.css.contains(property),
            "missing {property} in {}",
            asset.css
        );
    }
    assert_eq!(asset.fallback_family.as_deref(), Some("Test Sans Fallback"));
}

#[test]
fn an_unknown_local_face_is_declined_out_loud() {
    // A project that asked for a fallback and got none has to be able to find
    // out why without reading this crate.
    let (_guard, dir) = temp();
    let source = dir.join("TestSans.ttf");
    std::fs::write(&source, sfnt(&tables(1000, 800, -200, 0, 500))).unwrap();
    let out = dir.join("out");

    let asset = self_host(&request(&source, &out, Some("Comic Sans MS"))).unwrap();

    assert!(asset.fallback.is_none());
    let reason = asset.fallback_declined.expect("a reason");
    assert!(reason.contains("Comic Sans MS"), "{reason}");
    assert!(reason.contains("Arial"), "{reason}");
    assert!(!asset.css.contains("size-adjust"), "{}", asset.css);
}

#[test]
fn a_family_name_cannot_break_out_of_the_css_string() {
    // The family comes from `uf.config.js` and from imports, so it is the
    // project's text. A quote in it must not end the string and let the rest
    // become declarations.
    let (_guard, dir) = temp();
    let source = dir.join("Odd.ttf");
    std::fs::write(&source, sfnt(&tables(1000, 800, -200, 0, 500))).unwrap();
    let out = dir.join("out");

    let mut req = request(&source, &out, None);
    req.family = r#"Evil";color:red;--x:""#;
    let asset = self_host(&req).unwrap();

    // The whole family, quotes and all, stays inside one CSS string: every
    // quote it contained is escaped, so `color:red` is a run of characters in
    // a font name rather than a declaration the page will apply.
    assert!(
        asset
            .css
            .contains(r#"font-family:"Evil\";color:red;--x:\"";"#),
        "{}",
        asset.css
    );
    assert!(
        !asset.css.contains(r#"x:"";color"#),
        "the string was closed early: {}",
        asset.css
    );
}

#[test]
fn a_truetype_collection_is_refused_rather_than_guessed_at() {
    // It holds several faces and does not say which one was meant. Picking
    // index zero would produce confident, wrong metrics.
    let bytes = b"ttcf\x00\x01\x00\x00\x00\x00\x00\x02".to_vec();
    let error = read_metrics(Utf8Path::new("Helvetica.ttc"), &bytes).unwrap_err();

    assert!(format!("{error}").contains("collection"), "{error}");
}

#[test]
fn a_woff2_length_that_overflows_is_refused() {
    // The directory's lengths decide where the next entry starts, and they
    // come from a file a dependency can write.
    let mut bytes = woff2(&tables(1000, 800, -200, 0, 500));
    // Overwrite the first directory entry's length with a five-byte run of
    // continuation bytes, which no valid encoding uses.
    bytes[49] = 0xff;
    bytes[50] = 0xff;
    bytes[51] = 0xff;
    bytes[52] = 0xff;
    bytes[53] = 0xff;

    assert!(read_metrics(Utf8Path::new("t.woff2"), &bytes).is_err());
}

#[test]
fn nothing_is_a_font() {
    assert!(read_metrics(Utf8Path::new("t.ttf"), b"").is_err());
    assert!(read_metrics(Utf8Path::new("t.ttf"), b"not a font at all").is_err());
}

/// The provenance of [`LOCAL_FACES`], checked rather than asserted.
///
/// Every number in that table was read out of one of these files by
/// [`read_metrics`]. This re-reads them and fails if the table has drifted —
/// which is how three of them were caught being wrong when they were first
/// written from memory.
///
/// The files are macOS's, so the test only runs where they are. That is not a
/// silently reduced suite: the numbers cannot be re-derived on a machine that
/// does not have the fonts, and the alternative is checking four megabytes of
/// licensed font binaries into the repository.
#[test]
fn the_local_face_table_matches_the_fonts_it_was_read_from() {
    let sources = [
        ("Arial", "/System/Library/Fonts/Supplemental/Arial.ttf"),
        (
            "Times New Roman",
            "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
        ),
        (
            "Courier New",
            "/System/Library/Fonts/Supplemental/Courier New.ttf",
        ),
    ];

    let mut checked = 0;
    for (name, path) in sources {
        let path = Utf8Path::new(path);
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let (_, metrics) = read_metrics(path, &bytes).unwrap();
        let face = local_face(name).expect("the table knows this face");
        assert_eq!(
            face.metrics, metrics,
            "LOCAL_FACES has drifted from {path} for {name}"
        );
        checked += 1;
    }

    // Every face in the table has to be one of the ones above: a face uf
    // cannot re-derive is a face uf should not be scaling.
    assert_eq!(LOCAL_FACES.len(), sources.len());
    if checked == 0 {
        eprintln!("none of the reference fonts are on this machine; table unverified here");
    }
}

/// A minimal but real SFNT, for tests in other modules that need a font file.
pub(crate) fn sfnt_fixture() -> Vec<u8> {
    sfnt(&tables(1000, 800, -200, 0, 500))
}

#[test]
fn a_font_hosted_as_supplied_is_one_face_and_says_nothing_about_subsetting() {
    let (_guard, dir) = temp();
    let source = dir.join("Inter.ttf");
    std::fs::write(&source, sfnt_fixture()).unwrap();
    let out = dir.join("out");

    let asset = self_host(&request(&source, &out, Some("Arial"))).unwrap();

    assert_eq!(asset.faces.len(), 1);
    assert_eq!(asset.faces[0].file, asset.file);
    assert!(asset.faces[0].unicode_range.is_none());
    assert!(asset.subset.is_none());
    assert!(asset.subset_declined.is_none());
    assert_eq!(asset.source_bytes, asset.bytes);
    assert!(asset.faces[0].preload);
}

#[test]
fn ranges_emit_one_file_per_script_with_its_own_unicode_range() {
    let (_guard, dir) = temp();
    let source = dir.join("Wide.ttf");
    std::fs::write(
        &source,
        crate::testfont::font_with(&['A', 'B', 'a', 'b', 'Ω', 'α', 'Д', 'д']),
    )
    .unwrap();
    let out = dir.join("out");

    let asset = self_host(&FontRequest {
        subset: crate::subset::SubsetMode::Ranges,
        ..request(&source, &out, Some("Arial"))
    })
    .unwrap();

    assert_eq!(asset.subset.as_deref(), Some("ranges"));
    assert!(asset.subset_declined.is_none());
    assert!(asset.faces.len() >= 3, "{:?}", asset.faces);
    for face in &asset.faces {
        assert!(
            out.join(&face.file).exists(),
            "{} was not written",
            face.file
        );
        assert!(face.unicode_range.is_some(), "{face:?}");
        assert_eq!(face.container, crate::font::FontContainer::Woff);
        assert!(
            asset.css.contains(&format!(
                "unicode-range:{};",
                face.unicode_range.as_ref().unwrap()
            )),
            "{}",
            asset.css
        );
    }
    // Every rule declares the same family: that is what makes the browser
    // treat them as one face and choose between the files by character.
    assert_eq!(
        asset.css.matches("font-family:\"Test Sans\"").count(),
        asset.faces.len()
    );
}

#[test]
fn exactly_one_face_is_preloaded_however_many_there_are() {
    // A preload per bucket downloads the whole family up front, which is the
    // one thing the split existed to stop.
    let (_guard, dir) = temp();
    let source = dir.join("Wide.ttf");
    std::fs::write(
        &source,
        crate::testfont::font_with(&['A', 'B', 'Ω', 'Д', '中']),
    )
    .unwrap();
    let out = dir.join("out");

    let asset = self_host(&FontRequest {
        subset: crate::subset::SubsetMode::Ranges,
        ..request(&source, &out, None)
    })
    .unwrap();

    assert_eq!(asset.faces.iter().filter(|face| face.preload).count(), 1);
    let preloaded = asset.faces.iter().find(|face| face.preload).unwrap();
    assert_eq!(preloaded.bucket.as_deref(), Some("latin"));
    assert_eq!(preloaded.file, asset.file);
}

#[test]
fn preload_off_marks_no_face_at_all() {
    let (_guard, dir) = temp();
    let source = dir.join("Inter.ttf");
    std::fs::write(&source, sfnt_fixture()).unwrap();
    let out = dir.join("out");

    let asset = self_host(&FontRequest {
        preload: false,
        ..request(&source, &out, None)
    })
    .unwrap();
    assert!(asset.faces.iter().all(|face| !face.preload));
    // And the manifest still names a primary file, because the stylesheet has
    // to point at one.
    assert_eq!(asset.file, asset.faces[0].file);
}

#[test]
fn a_woff2_that_cannot_be_subsetted_is_still_self_hosted() {
    // A refusal is a sentence, not a build failure: the font works, and the
    // project is told it is the whole font and what to point at instead.
    let (_guard, dir) = temp();
    let source = dir.join("Inter.woff2");
    std::fs::write(&source, woff2(&tables(1000, 800, -200, 0, 500))).unwrap();
    let out = dir.join("out");

    let asset = self_host(&FontRequest {
        subset: crate::subset::SubsetMode::Ranges,
        ..request(&source, &out, Some("Arial"))
    })
    .unwrap();

    assert!(asset.subset.is_none());
    let reason = asset.subset_declined.expect("a refusal has to say why");
    assert!(reason.contains("WOFF2"), "{reason}");
    assert!(reason.contains(".ttf"), "{reason}");
    assert_eq!(asset.faces.len(), 1);
    assert!(out.join(&asset.file).exists());
    assert!(asset.css.contains("@font-face"));
}

#[test]
fn a_woff_round_trips_through_the_container_uf_writes() {
    // `pack_woff` is what a subsetted face is emitted as, and `woff_tables` is
    // what reads one. They are each other's only check outside a browser.
    let sfnt = crate::testfont::font_with(&['A', 'B', 'C']);
    let woff = crate::font::pack_woff(&sfnt).unwrap();
    assert_eq!(&woff[0..4], b"wOFF");

    let path = Utf8PathBuf::from("round.woff");
    let (container, metrics) = read_metrics(&path, &woff).unwrap();
    assert_eq!(container, FontContainer::Woff);
    let (_, original) = read_metrics(&Utf8PathBuf::from("round.ttf"), &sfnt).unwrap();
    assert_eq!(metrics, original);
}

#[test]
fn a_font_from_a_woff_container_can_be_flattened_and_subsetted() {
    let sfnt = crate::testfont::font_with(&['A', 'B', 'Ω']);
    let woff = crate::font::pack_woff(&sfnt).unwrap();
    let path = Utf8PathBuf::from("wide.woff");
    let flattened = crate::font::to_sfnt(&path, &woff).unwrap();
    let plan = crate::subset::plan(&flattened, &crate::subset::SubsetMode::Ranges).unwrap();
    assert!(plan.faces.iter().any(|face| face.bucket == "greek"));
}
