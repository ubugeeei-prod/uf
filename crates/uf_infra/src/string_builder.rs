//! Build owned compact strings or write directly into a caller's buffer.
//!
//! Prefer `push_str` for plain concatenation. Use `append!` when formatting is
//! necessary and a buffer already exists; `cstr!` keeps short owned results
//! inline. Converting the result back to `String` still allocates and belongs
//! at an API boundary, not inside a repeated append loop.

/// Format into a compact owned string without an intermediate `String`.
#[macro_export]
macro_rules! cstr {
    ($($arg:tt)*) => {
        $crate::compact_format(::core::format_args!($($arg)*))
    };
}

/// Append formatted text directly, preserving the destination's allocation.
///
/// Panics if a formatting implementation returns an error, like `format!`.
#[macro_export]
macro_rules! append {
    ($buffer:expr, $($arg:tt)*) => {{
        use ::core::fmt::Write as _;
        write!($buffer, $($arg)*).expect("formatting into a string failed")
    }};
}

/// Preserve an existing String API without duplicating the compact-to-String
/// conversion at each compatibility boundary. Inline values still allocate.
#[inline(never)]
pub fn into_string(value: crate::CompactString) -> String {
    value.into_string()
}

/// Share the formatting writer between call sites while writing into compact
/// storage directly. This avoids wrapping Arguments in another Display format.
#[inline(never)]
pub fn compact_format(arguments: core::fmt::Arguments<'_>) -> crate::CompactString {
    use core::fmt::Write as _;
    if let Some(text) = arguments.as_str() {
        return crate::CompactString::new(text);
    }
    let mut output = crate::CompactString::const_new("");
    output
        .write_fmt(arguments)
        .expect("formatting into a string failed");
    output
}
