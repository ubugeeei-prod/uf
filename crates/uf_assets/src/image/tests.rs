use camino::Utf8PathBuf;

use super::*;

/// A photograph-shaped source: JPEG, and noisy enough that a lossless
/// re-encode cannot win.
///
/// The noise is deterministic — a cheap linear congruential sequence — so a
/// test that depends on the encoded sizes gets the same numbers on every
/// machine and in every run.
fn photograph(dir: &Utf8Path, width: u32, height: u32) -> Utf8PathBuf {
    let mut state: u32 = 0x1234_5678;
    let mut buffer = image::RgbImage::new(width, height);
    for pixel in buffer.pixels_mut() {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let bytes = state.to_le_bytes();
        *pixel = image::Rgb([bytes[1], bytes[2], bytes[3]]);
    }
    let path = dir.join("photo.jpg");
    image::DynamicImage::ImageRgb8(buffer)
        .save_with_format(&path, image::ImageFormat::Jpeg)
        .unwrap();
    path
}

/// A graphic-shaped source: PNG, smoothly varying, which is what this build's
/// lossless WebP encoder is actually good at.
///
/// A gradient rather than the flat blocks that were here first, and the change
/// was made by measurement rather than by taste. Hard-edged blocks *do* favour
/// WebP at the source's own width — a 512px checkerboard came out six times
/// smaller — and lose to PNG badly once resampled, because a Lanczos kernel
/// turns every edge into a ring of near-colours and this encoder handles those
/// far worse than PNG's adaptive filtering does. A gradient wins at every
/// width, which is what this fixture is for. The one that loses at some widths
/// and not others is the case
/// `an_alternative_format_is_all_the_rungs_or_none_of_them` covers.
fn graphic(dir: &Utf8Path, width: u32, height: u32) -> Utf8PathBuf {
    let mut buffer = image::RgbaImage::new(width, height);
    for (x, y, pixel) in buffer.enumerate_pixels_mut() {
        let red = (u64::from(x) * 255 / u64::from(width.max(1))) as u8;
        let green = (u64::from(y) * 255 / u64::from(height.max(1))) as u8;
        *pixel = image::Rgba([red, green, 128, 255]);
    }
    let path = dir.join("graphic.png");
    image::DynamicImage::ImageRgba8(buffer)
        .save_with_format(&path, image::ImageFormat::Png)
        .unwrap();
    path
}

/// A source with hard edges, which this build's WebP encoder handles well
/// unresampled and very badly once resampled.
fn hard_edged(dir: &Utf8Path, width: u32, height: u32) -> Utf8PathBuf {
    let mut buffer = image::RgbaImage::new(width, height);
    for (x, y, pixel) in buffer.enumerate_pixels_mut() {
        let block = ((x / 64) + (y / 64)) % 3;
        *pixel = match block {
            0 => image::Rgba([12, 74, 156, 255]),
            1 => image::Rgba([243, 244, 246, 255]),
            _ => image::Rgba([220, 38, 38, 255]),
        };
    }
    let path = dir.join("edges.png");
    image::DynamicImage::ImageRgba8(buffer)
        .save_with_format(&path, image::ImageFormat::Png)
        .unwrap();
    path
}

fn out_dir(dir: &Utf8Path) -> Utf8PathBuf {
    dir.join("out")
}

fn temp() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    (dir, path)
}

#[test]
fn every_declared_width_is_on_disk_at_that_width() {
    // The claim the whole pipeline rests on. Not "a variant was recorded at
    // 640" — the file exists, and decoding it back gives 640 pixels. A resize
    // that does not resize would pass a test that only read the manifest.
    let (_guard, dir) = temp();
    let source = photograph(&dir, 1600, 1000);
    let out = out_dir(&dir);

    let asset = transform(&ImageRequest {
        source: &source,
        widths: &[320, 640, 960],
        quality: 75,
        blur: false,
        out_dir: &out,
    })
    .unwrap();

    assert_eq!(asset.width, Some(1600));
    assert_eq!(asset.height, Some(1000));

    let jpegs = asset.in_format(Emitted::Jpeg);
    let widths: Vec<u32> = jpegs.iter().map(|variant| variant.width).collect();
    assert_eq!(widths, vec![320, 640, 960, 1600], "{asset:?}");

    for variant in jpegs {
        let path = out.join(&variant.file);
        let bytes = std::fs::read(&path).unwrap_or_else(|_| panic!("{path} was not written"));
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!(
            (decoded.width(), decoded.height()),
            (variant.width, variant.height),
            "{} is not the size it claims",
            variant.file
        );
        assert_eq!(bytes.len() as u64, variant.bytes, "{}", variant.file);
    }
}

#[test]
fn a_narrower_variant_really_is_a_smaller_file() {
    // The point of the exercise. A `srcset` that offers a phone a 320px URL
    // which weighs what the 1600px one weighs has cost the reader a download
    // and saved nothing.
    let (_guard, dir) = temp();
    let source = photograph(&dir, 1600, 1000);
    let out = out_dir(&dir);

    let asset = transform(&ImageRequest {
        source: &source,
        widths: &[320],
        quality: 75,
        blur: false,
        out_dir: &out,
    })
    .unwrap();

    let jpegs = asset.in_format(Emitted::Jpeg);
    let narrow = jpegs.first().unwrap();
    let wide = jpegs.last().unwrap();

    assert_eq!(narrow.width, 320);
    assert_eq!(wide.width, 1600);
    assert!(
        narrow.bytes * 4 < wide.bytes,
        "320px is {} bytes and 1600px is {} — the resize did nothing useful",
        narrow.bytes,
        wide.bytes
    );
}

#[test]
fn uf_never_upscales() {
    // A 400px source asked for 1920 must not be stretched to 1920: the file
    // gets bigger, the picture gets worse, and the reader pays for both.
    let (_guard, dir) = temp();
    let source = graphic(&dir, 400, 300);
    let out = out_dir(&dir);

    let asset = transform(&ImageRequest {
        source: &source,
        widths: &[640, 1080, 1920],
        quality: 75,
        blur: false,
        out_dir: &out,
    })
    .unwrap();

    let widest = asset
        .variants
        .iter()
        .map(|variant| variant.width)
        .max()
        .unwrap();
    assert_eq!(widest, 400, "{asset:?}");
    // And there is still something to serve, which is the reason the source's
    // own width is always among the targets.
    assert!(!asset.variants.is_empty());
}

#[test]
fn a_flat_graphic_gets_the_webp_because_the_webp_is_smaller() {
    let (_guard, dir) = temp();
    let source = graphic(&dir, 512, 512);
    let out = out_dir(&dir);

    let asset = transform(&ImageRequest {
        source: &source,
        widths: &[256],
        quality: 75,
        blur: false,
        out_dir: &out,
    })
    .unwrap();

    let webps = asset.in_format(Emitted::Webp);
    assert!(!webps.is_empty(), "no webp emitted: {asset:?}");

    for webp in &webps {
        let png = asset
            .in_format(Emitted::Png)
            .into_iter()
            .find(|variant| variant.width == webp.width)
            .expect("a fallback at the same width");
        assert!(
            webp.bytes < png.bytes,
            "kept a {} byte webp beside a {} byte png",
            webp.bytes,
            png.bytes
        );
        // The bytes on disk are a real WebP, not a renamed PNG.
        let bytes = std::fs::read(out.join(&webp.file)).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF", "{}", webp.file);
        assert_eq!(&bytes[8..12], b"WEBP", "{}", webp.file);
    }
}

#[test]
fn a_photograph_does_not_get_a_lossless_webp_that_would_be_larger() {
    // The honest half of "modern formats with a fallback". Without a lossy
    // WebP or an AVIF encoder there is nothing here that beats the JPEG, and
    // emitting one anyway would make the page slower while the markup claimed
    // an optimisation. What uf does instead is measure and say so.
    let (_guard, dir) = temp();
    let source = photograph(&dir, 800, 600);
    let out = out_dir(&dir);

    let asset = transform(&ImageRequest {
        source: &source,
        widths: &[400],
        quality: 75,
        blur: false,
        out_dir: &out,
    })
    .unwrap();

    assert!(
        asset.in_format(Emitted::Webp).is_empty(),
        "a lossless webp of a photograph was kept: {asset:?}"
    );
    assert!(
        !asset.declined.is_empty(),
        "the rejection has to be reported, not silent"
    );
    for declined in &asset.declined {
        assert_eq!(declined.format, Emitted::Webp);
        assert!(
            declined.bytes > declined.fallback_bytes,
            "declined a webp that was actually smaller: {declined:?}"
        );
    }
    // Nothing was written for a format that was not kept.
    let emitted: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        !emitted.iter().any(|name| name.ends_with(".webp")),
        "{emitted:?}"
    );
}

#[test]
fn the_blur_placeholder_is_a_real_image_and_a_small_one() {
    let (_guard, dir) = temp();
    let source = photograph(&dir, 1200, 800);
    let out = out_dir(&dir);

    let asset = transform(&ImageRequest {
        source: &source,
        widths: &[600],
        quality: 75,
        blur: true,
        out_dir: &out,
    })
    .unwrap();

    let blur = asset.blur.expect("a blur placeholder was asked for");
    let (header, payload) = blur.split_once(";base64,").unwrap();
    assert!(header.starts_with("data:image/"), "{header}");

    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!(decoded.width(), BLUR_WIDTH);
    // It is inlined into every document that renders the image, so its weight
    // is paid on every page load rather than once.
    assert!(blur.len() < 2048, "the placeholder is {} bytes", blur.len());
}

#[test]
fn an_svg_is_served_as_written_and_says_so() {
    // Not a failure: an SVG is already resolution independent, so rasterising
    // it at a set of widths would be the wrong answer rather than a missing
    // one. What matters is that the caller can tell this apart from "uf
    // resized it and there was only one size".
    let (_guard, dir) = temp();
    let source = dir.join("mark.svg");
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><rect width="8" height="8"/></svg>"#;
    std::fs::write(&source, svg).unwrap();
    let out = out_dir(&dir);

    let asset = transform(&ImageRequest {
        source: &source,
        widths: &[640],
        quality: 75,
        blur: true,
        out_dir: &out,
    })
    .unwrap();

    assert_eq!(asset.format, Emitted::Original);
    assert_eq!(asset.width, None, "uf must not invent an intrinsic size");
    let note = asset.note.expect("a passthrough has to explain itself");
    assert!(note.contains("resolution independent"), "{note}");

    let variant = &asset.variants[0];
    assert_eq!(std::fs::read(out.join(&variant.file)).unwrap(), svg);
    assert_eq!(variant.mime, "image/svg+xml");
}

#[test]
fn a_format_with_no_decoder_is_passed_through_with_the_reason() {
    let (_guard, dir) = temp();
    let source = dir.join("frames.gif");
    // A tiny but real GIF header; uf has no GIF decoder in this build.
    std::fs::write(&source, b"GIF89a\x01\x00\x01\x00\x00\x00\x00;").unwrap();
    let out = out_dir(&dir);

    let asset = transform(&ImageRequest {
        source: &source,
        widths: &[640],
        quality: 75,
        blur: false,
        out_dir: &out,
    })
    .unwrap();

    assert_eq!(asset.format, Emitted::Original);
    let note = asset.note.expect("a passthrough has to explain itself");
    assert!(note.contains("no decoder"), "{note}");
    assert!(note.contains("not resized"), "{note}");
}

#[test]
fn the_fallback_format_is_offered_last() {
    // A browser takes the first `<source>` it understands, so the format every
    // browser understands must not be offered before the ones that are
    // smaller.
    let (_guard, dir) = temp();
    let source = graphic(&dir, 512, 512);
    let out = out_dir(&dir);

    let asset = transform(&ImageRequest {
        source: &source,
        widths: &[256],
        quality: 75,
        blur: false,
        out_dir: &out,
    })
    .unwrap();

    let formats = asset.formats();
    assert_eq!(formats.last(), Some(&Emitted::Png), "{formats:?}");
    assert!(formats.contains(&Emitted::Webp), "{formats:?}");
}

#[test]
fn two_runs_of_the_same_source_write_the_same_names() {
    // A rebuild that changed nothing must not invalidate a reader's cache.
    let (_guard, dir) = temp();
    let source = graphic(&dir, 300, 200);
    let out = out_dir(&dir);
    let request = ImageRequest {
        source: &source,
        widths: &[150],
        quality: 75,
        blur: false,
        out_dir: &out,
    };

    let first = transform(&request).unwrap();
    let second = transform(&request).unwrap();

    assert_eq!(first, second);
}

#[test]
fn changing_the_quality_changes_the_name() {
    // Same source, different bytes on disk: a name that did not move would
    // serve the old encoding forever.
    let (_guard, dir) = temp();
    let source = photograph(&dir, 200, 200);
    let out = out_dir(&dir);
    let at = |quality: u8| {
        transform(&ImageRequest {
            source: &source,
            widths: &[100],
            quality,
            blur: false,
            out_dir: &out,
        })
        .unwrap()
        .in_format(Emitted::Jpeg)
        .first()
        .unwrap()
        .file
        .clone()
    };

    assert_ne!(at(40), at(90));
}

#[test]
fn the_aspect_ratio_survives_the_resize() {
    // The ratio is what reserves the right box. A variant that is 640 wide and
    // the wrong height moves the page exactly as much as no dimensions at all.
    let (_guard, dir) = temp();
    let source = photograph(&dir, 1500, 500);
    let out = out_dir(&dir);

    let asset = transform(&ImageRequest {
        source: &source,
        widths: &[600, 300],
        quality: 75,
        blur: false,
        out_dir: &out,
    })
    .unwrap();

    for variant in &asset.variants {
        assert_eq!(
            variant.height,
            variant.width / 3,
            "{} is {}x{}",
            variant.file,
            variant.width,
            variant.height
        );
    }
}

#[test]
fn a_one_pixel_tall_result_is_one_pixel_rather_than_none() {
    // An encoder handed a zero dimension fails, and a very wide banner scaled
    // to a thumbnail is exactly where the rounding goes to zero.
    assert_eq!(scaled_height(2000, 3, 16), 1);
    assert_eq!(scaled_height(1000, 1000, 1), 1);
}

#[test]
fn a_webp_source_is_measured_even_though_it_is_not_re_encoded() {
    // Passing it through is the right answer — the only WebP this build can
    // write is lossless, which for an already-compressed source is bigger than
    // the file the author has. Refusing to *measure* it would be a separate
    // and unnecessary loss: the page needs the intrinsic size to reserve the
    // box, and uf has a decoder that can read it.
    let (_guard, dir) = temp();
    let png = graphic(&dir, 300, 200);
    let decoded = image::open(&png).unwrap();
    let source = dir.join("already.webp");
    let mut bytes = Vec::new();
    let rgb = decoded.to_rgb8();
    image::codecs::webp::WebPEncoder::new_lossless(&mut bytes)
        .write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    std::fs::write(&source, &bytes).unwrap();
    let out = out_dir(&dir);

    let asset = transform(&ImageRequest {
        source: &source,
        widths: &[150],
        quality: 75,
        blur: false,
        out_dir: &out,
    })
    .unwrap();

    assert_eq!(asset.format, Emitted::Original);
    assert_eq!(asset.width, Some(300), "{asset:?}");
    assert_eq!(asset.height, Some(200), "{asset:?}");
    let note = asset.note.expect("a passthrough has to explain itself");
    assert!(note.contains("lossless"), "{note}");
    // The bytes are the author's, unchanged.
    assert_eq!(
        std::fs::read(out.join(&asset.variants[0].file)).unwrap(),
        bytes
    );
}

#[test]
fn an_alternative_format_is_all_the_rungs_or_none_of_them() {
    // A `<picture>` offers a `<source>` as an equivalent rendering of the whole
    // image, and a browser that takes one then chooses a width from *that*
    // source's `srcset` alone. So a WebP ladder missing its narrow rungs does
    // not mean "prefer WebP where it wins" — it means a phone that picked the
    // WebP source downloads a wide one.
    //
    // This source is the case that makes the difference visible: hard edges
    // compress beautifully as lossless WebP at the source's own width and
    // catastrophically once resampled, so a per-width rule would emit exactly
    // the broken ladder above.
    let (_guard, dir) = temp();
    let source = hard_edged(&dir, 512, 512);
    let out = out_dir(&dir);

    let asset = transform(&ImageRequest {
        source: &source,
        widths: &[128, 256],
        quality: 75,
        blur: false,
        out_dir: &out,
    })
    .unwrap();

    let webps = asset.in_format(Emitted::Webp);
    let fallbacks = asset.in_format(Emitted::Png);
    assert!(!fallbacks.is_empty());
    assert!(
        webps.is_empty() || webps.len() == fallbacks.len(),
        "a partial ladder: {} webp rungs beside {} png rungs",
        webps.len(),
        fallbacks.len()
    );

    // Whatever was decided, only files that are referenced were written.
    let on_disk: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(on_disk.len(), asset.variants.len(), "{on_disk:?}");

    // And every width that lost is reported with the numbers that decided it.
    if webps.is_empty() {
        assert_eq!(
            asset.declined.len(),
            fallbacks.len(),
            "{:?}",
            asset.declined
        );
        for entry in &asset.declined {
            assert!(entry.fallback_bytes > 0, "{entry:?}");
        }
    }
}

#[test]
fn an_image_with_holes_in_it_gets_no_placeholder() {
    // The placeholder is painted *under* the image, which is what makes it
    // need no element and cost nothing to remove. Under an opaque photograph
    // it disappears the moment the real bytes decode; under a logo with a
    // transparent background it stays visible through it forever.
    let (_guard, dir) = temp();
    let source = dir.join("logo.png");
    let mut buffer = image::RgbaImage::new(64, 64);
    for (x, y, pixel) in buffer.enumerate_pixels_mut() {
        let inside = x > 16 && x < 48 && y > 16 && y < 48;
        *pixel = if inside {
            image::Rgba([37, 99, 235, 255])
        } else {
            image::Rgba([0, 0, 0, 0])
        };
    }
    image::DynamicImage::ImageRgba8(buffer)
        .save_with_format(&source, image::ImageFormat::Png)
        .unwrap();
    let out = out_dir(&dir);

    let asset = transform(&ImageRequest {
        source: &source,
        widths: &[32],
        quality: 75,
        blur: true,
        out_dir: &out,
    })
    .unwrap();

    assert!(asset.blur.is_none(), "{asset:?}");
    // Everything else about it is unchanged: it is still resized, and the
    // alpha channel survives.
    assert_eq!(asset.width, Some(64));
    assert!(asset.variants.iter().any(|variant| variant.width == 32));
    let widest = asset.in_format(Emitted::Png).pop().unwrap().file.clone();
    let decoded = image::open(out.join(&widest)).unwrap();
    assert!(
        decoded.color().has_alpha(),
        "the transparency was flattened"
    );
}
