//! Which of `libdefs/vite-assets.js`'s shapes a specifier has, if any.
//!
//! Flow's `declare module` takes a string literal, so there is no
//! `declare module "*.css"` and the question "what is `./logo.svg?url`" cannot
//! be asked in a libdef. It is asked here instead, and answered with the name
//! of a module that *is* declared there — so the mapping is a decision about a
//! path, in Rust, and what the answer is stays JavaScript somebody can read.
//!
//! This runs only for a relative specifier the batch has no file for, which is
//! every non-Flow file by construction: `uf check` is handed `.js` sources and
//! `package.json`, so a stylesheet was never going to be in it.

/// The declared module a Vite asset specifier resolves to.
///
/// `None` for anything this does not recognise, which keeps
/// [`super::project::ProjectModules::unchecked`] as the answer for a `./x.js`
/// that is simply missing — a typo in a relative import must stay visible.
pub(super) fn declared_module_for(specifier: &str) -> Option<&'static str> {
    let (path, query) = match specifier.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (specifier, None),
    };

    // The query wins over the extension, because that is Vite's rule:
    // `./a.css?raw` is a string of CSS source and not a stylesheet. Flags are
    // `&`-separated and order is not promised, so each is looked for rather
    // than the whole query matched.
    if let Some(query) = query {
        let flags: Vec<&str> = query.split('&').collect();
        // `?worker&inline` is still a worker; the inlining is how it is
        // delivered, not what it is.
        if flags.contains(&"sharedworker") {
            return Some("uf$asset$shared_worker");
        }
        if flags.contains(&"worker") {
            return Some("uf$asset$worker");
        }
        if flags.contains(&"raw") {
            return Some("uf$asset$raw");
        }
        if flags.contains(&"url") || flags.contains(&"inline") || flags.contains(&"no-inline") {
            return Some("uf$asset$url");
        }
    }

    let lowercase = path.to_ascii_lowercase();
    // `.module.css` before `.css`, and by the compound suffix rather than by
    // splitting on the last dot: `a.module.css` and `a.css` differ by what is
    // in front of the extension, which is the whole convention.
    if STYLE_EXTENSIONS
        .iter()
        .any(|extension| lowercase.ends_with(&format!(".module{extension}")))
    {
        return Some("uf$asset$css_module");
    }
    if STYLE_EXTENSIONS
        .iter()
        .any(|extension| lowercase.ends_with(extension))
    {
        return Some("uf$asset$css");
    }
    if ASSET_EXTENSIONS
        .iter()
        .any(|extension| lowercase.ends_with(extension))
    {
        return Some("uf$asset$url");
    }
    None
}

/// The stylesheet languages Vite compiles, which is its own list.
///
/// `.css` last would be wrong: `.postcss` ends with neither, and `.pcss` ends
/// with `.pcss`. They are all matched as whole suffixes, so the order here is
/// only for reading.
const STYLE_EXTENSIONS: &[&str] = &[
    ".css", ".scss", ".sass", ".less", ".styl", ".stylus", ".pcss", ".postcss", ".sss",
];

/// Vite's default `assetsInclude`, which is what a build hashes into
/// `dist/assets` and hands back as a URL.
///
/// Not a guess: this is the list in Vite's `constants.ts` under
/// `KNOWN_ASSET_TYPES`, minus the ones uf resolves for itself. A project that
/// adds to `assetsInclude` adds a type uf does not know about, and that import
/// goes back to being reported as untyped — which is the honest answer, and
/// ubugeeei-prod/uf#264 is where reading the project's own list is tracked.
const ASSET_EXTENSIONS: &[&str] = &[
    // images
    ".apng",
    ".bmp",
    ".png",
    ".jpe",
    ".jpeg",
    ".jpg",
    ".jfif",
    ".pjpeg",
    ".pjp",
    ".gif",
    ".svg",
    ".ico",
    ".webp",
    ".avif",
    ".cur",
    ".jxl", // media
    ".mp4",
    ".webm",
    ".ogg",
    ".mp3",
    ".wav",
    ".flac",
    ".aac",
    ".opus",
    ".mov",
    ".m4a",
    ".vtt",
    // fonts
    ".woff",
    ".woff2",
    ".eot",
    ".ttf",
    ".otf", // other
    ".webmanifest",
    ".pdf",
    ".txt",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_decides_the_shape_before_the_extension_does() {
        // Vite's rule, and the reason these are separate declarations rather
        // than options on one.
        assert_eq!(declared_module_for("./a.css?raw"), Some("uf$asset$raw"));
        assert_eq!(declared_module_for("./logo.svg?url"), Some("uf$asset$url"));
        assert_eq!(
            declared_module_for("./a.module.css?inline"),
            Some("uf$asset$url")
        );
    }

    #[test]
    fn a_worker_is_a_worker_however_it_is_delivered() {
        assert_eq!(
            declared_module_for("./w.js?worker"),
            Some("uf$asset$worker")
        );
        assert_eq!(
            declared_module_for("./w.js?worker&inline"),
            Some("uf$asset$worker")
        );
        assert_eq!(
            declared_module_for("./w.js?sharedworker"),
            Some("uf$asset$shared_worker")
        );
    }

    #[test]
    fn a_css_module_is_told_from_a_stylesheet_by_what_is_in_front_of_it() {
        assert_eq!(
            declared_module_for("./a.module.css"),
            Some("uf$asset$css_module")
        );
        assert_eq!(declared_module_for("./a.css"), Some("uf$asset$css"));
        assert_eq!(
            declared_module_for("./a.module.scss"),
            Some("uf$asset$css_module")
        );
        // Vite's `cssModuleRE` is `\.module` followed by the language, so the
        // dot in front is required: a file *called* `module.css` is a plain
        // stylesheet, and so is `amodule.css`. Checked against the regular
        // expression in Vite 8's `node.js`, not assumed — the first version of
        // this test asserted the opposite and was wrong.
        assert_eq!(declared_module_for("./module.css"), Some("uf$asset$css"));
        assert_eq!(declared_module_for("./amodule.css"), Some("uf$asset$css"));
    }

    #[test]
    fn an_asset_is_a_url_and_a_missing_module_is_still_missing() {
        assert_eq!(declared_module_for("./logo.png"), Some("uf$asset$url"));
        assert_eq!(declared_module_for("./Inter.woff2"), Some("uf$asset$url"));
        assert_eq!(declared_module_for("./LOGO.PNG"), Some("uf$asset$url"));

        // The half that must not change: a relative import of a module that is
        // not there stays untyped and reported, because a typo has to be
        // visible.
        assert_eq!(declared_module_for("./missing.js"), None);
        assert_eq!(declared_module_for("../outside.js"), None);
        assert_eq!(declared_module_for("./data.json"), None);
    }
}
