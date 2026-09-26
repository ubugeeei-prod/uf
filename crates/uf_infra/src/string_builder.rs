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
        $crate::format_compact!($($arg)*)
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
