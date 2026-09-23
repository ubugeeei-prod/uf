use super::*;

/// An opaque photograph-shaped JPEG, deterministic noise over a gradient.
fn jpeg(width: u32, height: u32) -> Vec<u8> {
    let mut state: u32 = 0x9e37_79b9;
    let mut buffer = image::RgbImage::new(width, height);
    for (x, y, pixel) in buffer.enumerate_pixels_mut() {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let noise = (state >> 28) as u8;
        let red = (u64::from(x) * 255 / u64::from(width.max(1))) as u8;
        let green = (u64::from(y) * 255 / u64::from(height.max(1))) as u8;
        *pixel = image::Rgb([red.saturating_add(noise), green, 96]);
    }
    let mut out = Vec::new();
    DynamicImage::ImageRgb8(buffer)
        .write_to(
            &mut std::io::Cursor::new(&mut out),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
    out
}

/// A PNG with a transparent half.
fn transparent_png(width: u32, height: u32) -> Vec<u8> {
    let mut buffer = image::RgbaImage::new(width, height);
    for (x, _, pixel) in buffer.enumerate_pixels_mut() {
        let alpha = if x < width / 2 { 0 } else { 255 };
        *pixel = image::Rgba([200, 40, 40, alpha]);
    }
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(buffer)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

fn decoded(bytes: &[u8]) -> (u32, u32) {
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .unwrap()
        .into_dimensions()
        .unwrap()
}

#[test]
fn a_browser_that_accepts_avif_gets_a_resized_avif() {
    let source = jpeg(96, 64);
    let answer = variant(&VariantRequest {
        bytes: &source,
        width: 48,
        quality: 60,
        avif: true,
    })
    .unwrap();
    assert_eq!(answer.format, VariantFormat::Avif);
    assert_eq!((answer.width, answer.height), (48, 32));
    // An AVIF is an ISO-BMFF file whose first box is `ftyp` with the `avif`
    // brand; the decoder is not compiled in, so the container is what is read.
    assert_eq!(&answer.bytes[4..8], b"ftyp");
    assert_eq!(&answer.bytes[8..12], b"avif");
}

#[test]
fn without_avif_a_jpeg_stays_a_jpeg_and_a_png_stays_a_png() {
    let photo = jpeg(96, 64);
    let answer = variant(&VariantRequest {
        bytes: &photo,
        width: 32,
        quality: 75,
        avif: false,
    })
    .unwrap();
    assert_eq!(answer.format, VariantFormat::Jpeg);
    assert_eq!(decoded(&answer.bytes), (32, 21));

    let logo = transparent_png(40, 20);
    let answer = variant(&VariantRequest {
        bytes: &logo,
        width: 20,
        quality: 75,
        avif: false,
    })
    .unwrap();
    assert_eq!(answer.format, VariantFormat::Png);
    assert_eq!(decoded(&answer.bytes), (20, 10));
}

#[test]
fn a_transparent_image_keeps_its_alpha_as_an_avif() {
    let logo = transparent_png(40, 20);
    let answer = variant(&VariantRequest {
        bytes: &logo,
        width: 40,
        quality: 60,
        avif: true,
    })
    .unwrap();
    assert_eq!(answer.format, VariantFormat::Avif);
    // An AVIF with transparency carries a second image item, the alpha plane,
    // declared by its auxiliary type URN.
    let alpha_urn = b"urn:mpeg:mpegB:cicp:systems:auxiliary:alpha";
    assert!(
        answer
            .bytes
            .windows(alpha_urn.len())
            .any(|window| window == alpha_urn),
        "the AVIF has no alpha plane"
    );
}

#[test]
fn it_never_upscales() {
    let source = jpeg(40, 30);
    let answer = variant(&VariantRequest {
        bytes: &source,
        width: 1200,
        quality: 75,
        avif: false,
    })
    .unwrap();
    assert_eq!((answer.width, answer.height), (40, 30));
}

#[test]
fn an_svg_is_refused_rather_than_passed_through() {
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#;
    let refused = variant(&VariantRequest {
        bytes: svg,
        width: 64,
        quality: 75,
        avif: true,
    })
    .unwrap_err();
    assert!(matches!(refused, VariantError::Unsupported), "{refused}");
}

#[test]
fn a_header_declaring_too_many_pixels_is_refused_before_decoding() {
    // A valid PNG signature and IHDR declaring 20000x20000, and an IDAT with
    // almost nothing in it: a decompression bomb's shape. The header is all
    // that is read — the decoder only needs to reach the first IDAT to say
    // how large the image claims to be.
    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&20_000u32.to_be_bytes());
    ihdr.extend_from_slice(&20_000u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(
        &mut png,
        b"IDAT",
        &[0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01],
    );
    chunk(&mut png, b"IEND", &[]);

    let refused = variant(&VariantRequest {
        bytes: &png,
        width: 640,
        quality: 75,
        avif: false,
    })
    .unwrap_err();
    assert!(
        matches!(
            refused,
            VariantError::TooManyPixels {
                width: 20_000,
                height: 20_000
            }
        ),
        "{refused}"
    );
}

#[test]
fn an_oversized_source_is_refused_before_it_is_parsed() {
    let bytes = vec![0u8; usize::try_from(MAX_VARIANT_SOURCE_BYTES).unwrap() + 1];
    let refused = variant(&VariantRequest {
        bytes: &bytes,
        width: 64,
        quality: 75,
        avif: false,
    })
    .unwrap_err();
    assert!(
        matches!(refused, VariantError::TooLarge { .. }),
        "{refused}"
    );
}

#[test]
fn widths_and_qualities_out_of_range_are_refused() {
    let source = jpeg(8, 8);
    for (width, quality) in [(0, 75), (MAX_VARIANT_WIDTH + 1, 75), (64, 0), (64, 101)] {
        assert!(
            variant(&VariantRequest {
                bytes: &source,
                width,
                quality,
                avif: false,
            })
            .is_err(),
            "{width}w at q{quality} was accepted"
        );
    }
}

/// Append one PNG chunk: length, type, data, CRC.
fn chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    png.extend_from_slice(&body);
    png.extend_from_slice(&crc32(&body).to_be_bytes());
}

/// CRC-32 (IEEE), for the hand-built PNG chunks above.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}
