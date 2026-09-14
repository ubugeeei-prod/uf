#![no_main]

// A declaration file is text a package author wrote, read on every
// `uf check` of every project that installs the package, so no input may
// panic the translation. The input is split into two files at the first NUL
// byte, so a relative import from one to the other is reachable too.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let (entry, other) = text.split_once('\0').unwrap_or((text, ""));
    let _ = uf_dts::translate(&["index.d.ts"], &mut |path| match path {
        "index.d.ts" => Some(entry.to_owned()),
        "other.d.ts" => Some(other.to_owned()),
        _ => None,
    });
});
