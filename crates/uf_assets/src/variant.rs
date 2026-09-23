//! One width of one image, encoded when a request asks for it.
//!
//! [`crate::image`] is the build-time pipeline: a file a module imported,
//! every declared width at once, written to disk under a content hash. This is
//! the other schedule — the request-time endpoint `@uniflowed/server/image`
//! answers for a remote or user-supplied image, whose URL nobody knew when the
//! project was built. It is handed bytes the server already fetched and bounded,
//! one width and one quality, and whether the browser accepts AVIF, and it
//! answers with one encoded file.
//!
//! # Why AVIF, and why here and not at build time
//!
//! The endpoint's reason to exist is a modern *lossy* format for a photograph
//! uf has never seen, and the only lossy modern encoder there is in pure Rust
//! is AV1's: `rav1e`, through `ravif`, through `image`'s `avif` feature with
//! its assembly turned off. Lossy WebP has no pure-Rust encoder under a licence
//! this repository can ship (the one that exists is AGPL), and `libwebp` is C.
//! So the decision [`crate::image`]'s documentation deferred is made here, and
//! made for the schedule that needs it: around forty more crates in the
//! lockfile, all pure Rust, no C toolchain and no native library.
//!
//! The build-time pipeline does not use it yet, and that is deliberate rather
//! than an oversight. It measures every candidate against the fallback at every
//! width and keeps a format only when it wins them all, and adding a candidate
//! changes what every existing project's build writes — a change worth its own
//! measurement, not one to make as a side effect of this endpoint.
//!
//! # What a request gets
//!
//! * **AVIF when the browser said it accepts it**, lossy at the requested
//!   quality, with an alpha plane when the source has transparency.
//! * **Otherwise the source's own family**: a JPEG for a JPEG, a PNG for a
//!   PNG, and for a WebP source a PNG when it has transparency and a JPEG when
//!   it does not — the fallback every browser takes.
//! * **Never wider than the source.** A 640px source asked for at 1200 comes
//!   back at 640, for the reason [`crate::image`] never upscales.
//! * **Upright.** An EXIF orientation is applied before resizing, because the
//!   re-encode drops the metadata that told a browser to rotate it — and with
//!   it the camera's GPS position, which is the right thing to drop from a
//!   file a server is about to hand to anybody who asks.
//!
//! # What it refuses
//!
//! Anything that is not a PNG, a JPEG or a WebP — by the bytes, not by a
//! header the origin wrote. An SVG in particular: served from an application's
//! own origin it is a document that can run script, which is why an image
//! proxy that passes one through is an XSS vector rather than a convenience.
//!
//! And anything over a bound, before the work the bound exists to prevent:
//! [`MAX_VARIANT_SOURCE_BYTES`] before anything is parsed,
//! [`MAX_VARIANT_PIXELS`] against the header before a buffer is allocated, and
//! the decoder itself under [`image::Limits`] as well, so a file whose header
//! lies about its size fails in the decoder rather than in the allocator.

use image::{DynamicImage, ImageDecoder as _, ImageEncoder as _, imageops::FilterType};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

/// Largest source the endpoint will decode, in bytes.
///
/// `@uniflowed/server/image` stops reading a response at the same number, so
/// a body over it never reaches this function; the check here is for a caller
/// that is not that endpoint.
pub const MAX_VARIANT_SOURCE_BYTES: u64 = 32 * 1024 * 1024;

/// Largest source the endpoint will decode, in pixels.
///
/// 64 megapixels: past any camera a CMS is likely to be fed, and a quarter of a
/// gibibyte decoded. Far below [`crate::image::MAX_SOURCE_PIXELS`], because
/// that bound is for a file in the project and this one is for a URL anybody
/// can put in a query string — every request under it can cost this much
/// memory, and several can be in flight.
pub const MAX_VARIANT_PIXELS: u64 = 64 * 1024 * 1024;

/// Widest variant the endpoint will produce.
///
/// The endpoint only asks for a width the project declared, so this is a
/// ceiling on a caller that is not the endpoint rather than a setting.
pub const MAX_VARIANT_WIDTH: u32 = 8192;

/// How hard `rav1e` works on an AVIF, from 1 (slowest) to 10.
///
/// 8, because this runs while somebody waits. The slowest settings buy a few
/// percent of file size for several times the time, and every variant is
/// encoded once per lifetime of its cache entry rather than once per build.
pub const AVIF_SPEED: u8 = 8;

/// The resampling filter.
///
/// Catmull-Rom rather than the Lanczos3 the build uses: nearly as sharp on a
/// downscale, and a third cheaper, which matters on a request path in a way it
/// does not at build time.
const RESIZE_FILTER: FilterType = FilterType::CatmullRom;

/// A format the endpoint answers with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VariantFormat {
    /// AVIF, lossy — the modern format, for a browser that accepts it.
    Avif,
    /// JPEG at the requested quality.
    Jpeg,
    /// PNG, lossless.
    Png,
}

impl VariantFormat {
    /// The media type, for `Content-Type`.
    #[must_use]
    pub const fn mime(self) -> &'static str {
        match self {
            Self::Avif => "image/avif",
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
        }
    }

    /// The file extension.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Avif => "avif",
            Self::Jpeg => "jpg",
            Self::Png => "png",
        }
    }
}

/// One variant a request asked for.
#[derive(Debug, Clone, Copy)]
pub struct VariantRequest<'a> {
    /// The source, as fetched.
    pub bytes: &'a [u8],
    /// The width asked for. The answer is never wider than the source.
    pub width: u32,
    /// Quality, 1–100, for the lossy encoders.
    pub quality: u8,
    /// Whether the browser accepts AVIF.
    pub avif: bool,
}

/// The encoded answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedVariant {
    /// The encoded file.
    pub bytes: Vec<u8>,
    /// Its format.
    pub format: VariantFormat,
    /// Its width in pixels.
    pub width: u32,
    /// Its height in pixels.
    pub height: u32,
}

/// A variant that could not be produced.
#[derive(Debug, thiserror::Error)]
pub enum VariantError {
    /// The source is larger than [`MAX_VARIANT_SOURCE_BYTES`].
    #[error("the source is {bytes} bytes, more than the {MAX_VARIANT_SOURCE_BYTES} byte limit")]
    TooLarge {
        /// Its size.
        bytes: u64,
    },
    /// The source is not a PNG, a JPEG or a WebP.
    #[error("the source is not a PNG, JPEG or WebP image")]
    Unsupported,
    /// The source's header declares more than [`MAX_VARIANT_PIXELS`].
    #[error("the source is {width}x{height}, more than the {MAX_VARIANT_PIXELS} pixel limit")]
    TooManyPixels {
        /// Its declared width.
        width: u32,
        /// Its declared height.
        height: u32,
    },
    /// The width asked for is zero or over [`MAX_VARIANT_WIDTH`].
    #[error("a width of {0} is outside 1..={MAX_VARIANT_WIDTH}")]
    Width(u32),
    /// The quality asked for is outside 1–100.
    #[error("a quality of {0} is outside 1..=100")]
    Quality(u8),
    /// The bytes did not decode.
    #[error("the source could not be decoded: {0}")]
    Decode(#[source] image::ImageError),
    /// The variant did not encode.
    #[error("the variant could not be encoded as {format}: {source}")]
    Encode {
        /// The format being written.
        format: &'static str,
        /// Underlying error.
        #[source]
        source: image::ImageError,
    },
}

/// Decode `request.bytes` once and encode it at `request.width`.
///
/// # Errors
///
/// [`VariantError`] when the request is out of range, the source is over a
/// bound or is not an image this crate decodes, or an encoder fails.
pub fn variant(request: &VariantRequest<'_>) -> Result<EncodedVariant, VariantError> {
    if request.width == 0 || request.width > MAX_VARIANT_WIDTH {
        return Err(VariantError::Width(request.width));
    }
    if !(1..=100).contains(&request.quality) {
        return Err(VariantError::Quality(request.quality));
    }
    let length = request.bytes.len() as u64;
    if length > MAX_VARIANT_SOURCE_BYTES {
        return Err(VariantError::TooLarge { bytes: length });
    }

    // From the bytes, never from a `Content-Type` somebody else's server
    // wrote: the decoder that matters is the one that can read what is there.
    let source_format = match image::guess_format(request.bytes) {
        Ok(
            format
            @ (image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP),
        ) => format,
        _ => return Err(VariantError::Unsupported),
    };

    let (declared_width, declared_height) = reader(request.bytes, source_format)
        .into_dimensions()
        .map_err(VariantError::Decode)?;
    if u64::from(declared_width) * u64::from(declared_height) > MAX_VARIANT_PIXELS {
        return Err(VariantError::TooManyPixels {
            width: declared_width,
            height: declared_height,
        });
    }

    let decoded = decode(request.bytes, source_format)?;
    let (source_width, source_height) = (decoded.width(), decoded.height());
    let width = request.width.min(source_width).max(1);
    let height = scaled_height(source_width, source_height, width);
    let scaled = if width == source_width {
        decoded
    } else {
        decoded.resize_exact(width, height, RESIZE_FILTER)
    };

    let alpha = has_alpha(&scaled);
    let format = if request.avif {
        VariantFormat::Avif
    } else {
        match source_format {
            image::ImageFormat::Jpeg => VariantFormat::Jpeg,
            image::ImageFormat::Png => VariantFormat::Png,
            _ if alpha => VariantFormat::Png,
            _ => VariantFormat::Jpeg,
        }
    };

    let bytes = encode(&scaled, format, request.quality, alpha)?;
    Ok(EncodedVariant {
        bytes,
        format,
        width: scaled.width(),
        height: scaled.height(),
    })
}

/// A reader over `bytes` that will not decode past the endpoint's bounds.
///
/// The header check in [`variant`] is the one that answers with a clear error;
/// these limits are the one that holds when a header is lying, because they
/// are enforced by the decoder as it allocates.
fn reader(bytes: &[u8], format: image::ImageFormat) -> image::ImageReader<std::io::Cursor<&[u8]>> {
    let mut limits = image::Limits::default();
    let side = u32::try_from(MAX_VARIANT_PIXELS).unwrap_or(u32::MAX);
    limits.max_image_width = Some(side);
    limits.max_image_height = Some(side);
    // Four bytes a pixel for the decoded buffer, and as much again for the
    // decoder's own working set, which for a progressive JPEG is a whole
    // second image of coefficients.
    limits.max_alloc = Some(MAX_VARIANT_PIXELS * 8);
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    reader.limits(limits);
    reader
}

/// Decode, with the EXIF orientation applied.
fn decode(bytes: &[u8], format: image::ImageFormat) -> Result<DynamicImage, VariantError> {
    let mut decoder = reader(bytes, format)
        .into_decoder()
        .map_err(VariantError::Decode)?;
    // A missing or unreadable orientation is not a reason to refuse the image:
    // it is what nearly every image has, and it means "already upright".
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut decoded = DynamicImage::from_decoder(decoder).map_err(VariantError::Decode)?;
    decoded.apply_orientation(orientation);
    Ok(decoded)
}

/// The height that keeps the source's aspect ratio at `width`, never zero.
fn scaled_height(source_width: u32, source_height: u32, width: u32) -> u32 {
    if source_width == 0 {
        return source_height.max(1);
    }
    let height = (u64::from(source_height) * u64::from(width) + u64::from(source_width) / 2)
        / u64::from(source_width);
    u32::try_from(height).unwrap_or(u32::MAX).max(1)
}

/// Whether any pixel is not fully opaque.
fn has_alpha(image: &DynamicImage) -> bool {
    if !image.color().has_alpha() {
        return false;
    }
    image.to_rgba8().pixels().any(|pixel| pixel.0[3] != u8::MAX)
}

/// Encode one scaled image.
fn encode(
    scaled: &DynamicImage,
    format: VariantFormat,
    quality: u8,
    alpha: bool,
) -> Result<Vec<u8>, VariantError> {
    let mut out = Vec::new();
    let failed = |source: image::ImageError| VariantError::Encode {
        format: format.extension(),
        source,
    };
    match format {
        VariantFormat::Avif => {
            let encoder = image::codecs::avif::AvifEncoder::new_with_speed_quality(
                &mut out, AVIF_SPEED, quality,
            );
            // No alpha plane for an opaque image: the encoder would spend
            // bits describing a plane that says nothing.
            if alpha {
                let rgba = scaled.to_rgba8();
                encoder
                    .write_image(
                        rgba.as_raw(),
                        rgba.width(),
                        rgba.height(),
                        image::ExtendedColorType::Rgba8,
                    )
                    .map_err(failed)?;
            } else {
                let rgb = scaled.to_rgb8();
                encoder
                    .write_image(
                        rgb.as_raw(),
                        rgb.width(),
                        rgb.height(),
                        image::ExtendedColorType::Rgb8,
                    )
                    .map_err(failed)?;
            }
        }
        VariantFormat::Jpeg => {
            let rgb = scaled.to_rgb8();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
                .write_image(
                    rgb.as_raw(),
                    rgb.width(),
                    rgb.height(),
                    image::ExtendedColorType::Rgb8,
                )
                .map_err(failed)?;
        }
        VariantFormat::Png => {
            let rgba = scaled.to_rgba8();
            image::codecs::png::PngEncoder::new_with_quality(
                &mut out,
                image::codecs::png::CompressionType::Default,
                image::codecs::png::FilterType::Adaptive,
            )
            .write_image(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
                image::ExtendedColorType::Rgba8,
            )
            .map_err(failed)?;
        }
    }
    Ok(out)
}
