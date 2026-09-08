//! An SVG as a component, and the sprite built from the ones a build reached.
//!
//! # The thing a runtime icon library cannot do
//!
//! There are three ways to ship icons today and all three waste something. A
//! component per icon duplicates the same `<svg>` wrapper in every module. A
//! sprite assembled by hand goes stale the moment somebody adds an icon. A
//! runtime library ships every icon it has, because at runtime nothing knows
//! which ones the application uses.
//!
//! A build does know. It resolved every import; the set of icons it reached is
//! exactly the set it resolved. So [`sprite`] assembles one `<symbol>` per
//! icon that was actually imported, an [`icon`] import evaluates to the symbol
//! id and the geometry a component needs, and every use of an icon is a
//! `<use>` element of about forty bytes rather than another copy of the path
//! data.
//!
//! # Why an import and not an extension
//!
//! `docs/red-lines.md` leaves `.svg` to Vite, and
//! `packages/host/assets.js` says why at length: claiming the extension would
//! take it from `vite-plugin-svgr` and everything like it, and a uf project
//! must be able to do what a Vite project can. So icons are reached through
//! uf's own namespace — `import Star from "uf:icon/star"` — resolved against a
//! directory the project names. That claims nothing that was not uf's already.
//!
//! # What is refused
//!
//! An icon file is markup, it is inlined into the document, and it can come
//! from a dependency. The dangerous constructs are refused by name rather than
//! stripped, because an icon that was silently edited renders differently from
//! the file in the repository and nobody finds out until it looks wrong:
//!
//! * `<script>` and `<foreignObject>`, which execute and which embed HTML;
//! * any `on…` attribute, which is a script in an attribute;
//! * any `javascript:` value;
//! * any reference out of the file — `href`, `xlink:href` or `src` that is not
//!   a `#fragment` — which is a request to a third party from inside a
//!   document that is supposed to be self-hosted.
//!
//! Everything else is passed through unchanged, with one rewrite: an `id`
//! inside an icon, and every `url(#…)` and `href="#…"` that refers to one, is
//! prefixed with the symbol's own id. Two icons that both define a gradient
//! called `a` are the normal case, and in one sprite the second would win for
//! both.

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use crate::name::hashed_name;

#[cfg(test)]
mod tests;

/// Largest SVG this module will read.
///
/// An icon is a few hundred bytes. The bound is on a file a dependency chose.
pub const MAX_ICON_BYTES: u64 = 1024 * 1024;

/// The prefix every symbol id carries.
///
/// A document may hold markup from anywhere; a symbol id that could collide
/// with an application's own element id would make `<use href="#name">` point
/// at whichever came first.
pub const SYMBOL_PREFIX: &str = "uf-icon-";

/// An icon that could not be used.
#[derive(Debug, thiserror::Error)]
pub enum IconError {
    /// The file could not be read.
    #[error("failed to read {path}: {source}")]
    Read {
        /// Path that failed.
        path: Utf8PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The file is larger than [`MAX_ICON_BYTES`].
    #[error("{path} is {bytes} bytes, larger than the {MAX_ICON_BYTES} byte limit for an icon")]
    TooLarge {
        /// Path that was too large.
        path: Utf8PathBuf,
        /// Its size.
        bytes: u64,
    },
    /// The file is not an SVG this module can turn into a symbol.
    #[error("{path} is not an SVG uf can make a symbol from: {reason}")]
    Malformed {
        /// Path that failed.
        path: Utf8PathBuf,
        /// What was wrong with it.
        reason: String,
    },
    /// The file contains something uf will not inline into a document.
    #[error(
        "{path} contains {found}, which uf will not inline into a document: {reason}. Remove it \
         from the file, or render this one as an ordinary image instead of an icon"
    )]
    Refused {
        /// Path that failed.
        path: Utf8PathBuf,
        /// What was found.
        found: String,
        /// Why it is refused.
        reason: &'static str,
    },
    /// The sprite could not be written.
    #[error("failed to write {path}: {source}")]
    Write {
        /// Path that failed.
        path: Utf8PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
}

/// What a project asked uf to do with one icon.
#[derive(Debug, Clone)]
pub struct IconRequest<'a> {
    /// The SVG file.
    pub source: &'a Utf8Path,
    /// The name it was imported under — `star` for `uf:icon/star`.
    pub name: &'a str,
}

/// One icon, as a symbol and the numbers a component needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IconAsset {
    /// The name it was imported under.
    pub name: String,
    /// The symbol's id, unique across the sprite.
    ///
    /// Content-hashed as well as named, so two icons with the same name from
    /// two directories do not collide and so an edited icon gets a new id
    /// rather than a stale sprite entry under the old one.
    pub id: String,
    /// The `viewBox` a `<use>` needs to be scaled correctly.
    pub view_box: String,
    /// The intrinsic width from the `viewBox`.
    pub width: f32,
    /// The intrinsic height from the `viewBox`.
    pub height: f32,
    /// The `<symbol>` element, ready to go in a sprite.
    pub symbol: String,
}

/// Read one SVG and turn it into a symbol.
///
/// # Errors
///
/// [`IconError`] when the file cannot be read, is over [`MAX_ICON_BYTES`], has
/// no usable `<svg>` root, or contains one of the constructs this module
/// refuses to inline.
pub fn icon(request: &IconRequest<'_>) -> Result<IconAsset, IconError> {
    let metadata = std::fs::metadata(request.source).map_err(|source| IconError::Read {
        path: request.source.to_owned(),
        source,
    })?;
    if metadata.len() > MAX_ICON_BYTES {
        return Err(IconError::TooLarge {
            path: request.source.to_owned(),
            bytes: metadata.len(),
        });
    }
    let raw = std::fs::read_to_string(request.source).map_err(|source| IconError::Read {
        path: request.source.to_owned(),
        source,
    })?;

    refuse_dangerous(request.source, &raw)?;

    let (attributes, inner) = root(request.source, &raw)?;
    let view_box = view_box(&attributes).ok_or_else(|| IconError::Malformed {
        path: request.source.to_owned(),
        reason: String::from(
            "it has neither a viewBox nor a width and height, so nothing can scale it",
        ),
    })?;
    let (width, height) = dimensions(&view_box).ok_or_else(|| IconError::Malformed {
        path: request.source.to_owned(),
        reason: format!("its viewBox {view_box:?} is not four numbers"),
    })?;

    // The digest covers the file's bytes and the name it was imported under,
    // so an edited icon gets a new symbol id and a sprite cached under the old
    // one cannot answer for it.
    let digest = hashed_name("i", raw.as_bytes(), &[request.name.as_bytes()], "x");
    let id = format!(
        "{SYMBOL_PREFIX}{}-{}",
        sanitise(request.name),
        digest.trim_start_matches("i.").trim_end_matches(".x")
    );
    let body = namespace_ids(&inner, &id);
    let symbol = format!(
        "<symbol id=\"{id}\" viewBox=\"{view_box}\"{carried}>{body}</symbol>",
        view_box = escape(&view_box),
        carried = carried(&attributes),
    );

    Ok(IconAsset {
        name: request.name.to_owned(),
        id,
        view_box,
        width,
        height,
        symbol,
    })
}

/// What a build asked uf to assemble.
#[derive(Debug, Clone)]
pub struct SpriteRequest<'a> {
    /// Every icon the build reached, in any order.
    pub icons: &'a [IconAsset],
    /// Where the sprite is written.
    pub out_dir: &'a Utf8Path,
}

/// The one sprite a build produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sprite {
    /// The emitted file's name, content-hashed.
    pub file: String,
    /// The markup, for a page that inlines it rather than fetching it.
    pub markup: String,
    /// Its size on disk.
    pub bytes: u64,
    /// How many symbols are in it.
    pub symbols: usize,
}

/// Assemble one sprite from the icons a build reached.
///
/// Sorted by id, so the same set of icons produces the same bytes and
/// therefore the same name whatever order the bundler resolved them in.
///
/// # Errors
///
/// [`IconError::Write`] when the sprite cannot be written.
pub fn sprite(request: &SpriteRequest<'_>) -> Result<Sprite, IconError> {
    let mut sorted: Vec<&IconAsset> = request.icons.iter().collect();
    sorted.sort_by(|left, right| left.id.cmp(&right.id));
    sorted.dedup_by(|left, right| left.id == right.id);

    let mut markup = String::from(
        // `aria-hidden` and `display:none` because the sprite itself is not
        // content: it is a definitions block, and a screen reader that
        // announced every symbol in it would read the whole icon set aloud
        // before the page.
        "<svg xmlns=\"http://www.w3.org/2000/svg\" aria-hidden=\"true\" \
         style=\"position:absolute;width:0;height:0;overflow:hidden\">",
    );
    for asset in &sorted {
        markup.push_str(&asset.symbol);
    }
    markup.push_str("</svg>");

    let file = hashed_name("icons", markup.as_bytes(), &[], "svg");
    std::fs::create_dir_all(request.out_dir).map_err(|source| IconError::Write {
        path: request.out_dir.to_owned(),
        source,
    })?;
    let target = request.out_dir.join(&file);
    std::fs::write(&target, &markup).map_err(|source| IconError::Write {
        path: target,
        source,
    })?;

    Ok(Sprite {
        file,
        bytes: markup.len() as u64,
        symbols: sorted.len(),
        markup,
    })
}

/// Refuse the constructs this module will not put in a document.
///
/// A scan of the whole text rather than of a parsed tree, deliberately: the
/// question is not "what does uf's parser think this file is" but "could a
/// browser find a script in it", and the two differ exactly where an attacker
/// would put one. Matching more than a strict parser would is the safe
/// direction — the cost of a false positive is a message naming the file.
fn refuse_dangerous(path: &Utf8Path, raw: &str) -> Result<(), IconError> {
    let lower = raw.to_ascii_lowercase();
    let refuse = |found: &str, reason: &'static str| IconError::Refused {
        path: path.to_owned(),
        found: found.to_owned(),
        reason,
    };
    for (needle, name, reason) in [
        ("<script", "a <script> element", "it would run in the page"),
        (
            "<foreignobject",
            "a <foreignObject> element",
            "it embeds arbitrary HTML inside the SVG",
        ),
        (
            "javascript:",
            "a javascript: URL",
            "it would run in the page",
        ),
    ] {
        if lower.contains(needle) {
            return Err(refuse(name, reason));
        }
    }
    // An event-handler attribute: ` on…=`. The leading space is what keeps
    // this off attributes that merely end in something like `version="on"`.
    //
    // Bytes throughout, never a string slice: an icon may carry any UTF-8 —
    // a `<title>` with an accent in it is an ordinary thing for a labelled
    // icon set to ship — and slicing a `str` at an offset that is not a code
    // point boundary panics rather than failing the import.
    let bytes = lower.as_bytes();
    for (index, window) in bytes.windows(3).enumerate() {
        if window != b" on" {
            continue;
        }
        let after = index + 3;
        let Some(length) = bytes[after..]
            .iter()
            .position(|byte| matches!(byte, b'=' | b'>' | b' '))
        else {
            continue;
        };
        let name = &bytes[after..after + length];
        if !name.is_empty()
            && bytes[after + length] == b'='
            && name.iter().all(u8::is_ascii_alphabetic)
        {
            return Err(refuse(
                &format!("the attribute on{}", String::from_utf8_lossy(name)),
                "an on… attribute is a script in an attribute",
            ));
        }
    }
    for attribute in ["href=", "xlink:href=", "src="] {
        let mut from = 0;
        while let Some(at) = lower[from..].find(attribute) {
            let start = from + at + attribute.len();
            let value = raw[start..]
                .trim_start()
                .trim_start_matches(['"', '\''])
                .trim_start();
            if !value.starts_with('#') {
                let end = value.find(['"', '\'', ' ', '>']).unwrap_or(0);
                return Err(refuse(
                    &format!("a reference to {:?}", &value[..end]),
                    "an icon is inlined into a self-hosted document and must not fetch anything",
                ));
            }
            from = start;
        }
    }
    Ok(())
}

/// The root `<svg>` element's attributes and its children, as text.
fn root(path: &Utf8Path, raw: &str) -> Result<(String, String), IconError> {
    let malformed = |reason: &str| IconError::Malformed {
        path: path.to_owned(),
        reason: reason.to_owned(),
    };
    let open = raw
        .find("<svg")
        .ok_or_else(|| malformed("it has no <svg> element"))?;
    let after = raw[open..]
        .find('>')
        .ok_or_else(|| malformed("its <svg> element is not closed"))?;
    // `<svg …/>` with no children: an icon that draws nothing, which is a
    // strange thing to import and not a malformed file. The slash is inside
    // the tag, so it is the attribute text that ends with it, not the `>`.
    let declared = raw[open + 4..open + after].trim_end();
    let self_closing = declared.ends_with('/');
    let attributes = declared.trim_end_matches('/').trim_end().to_owned();
    if self_closing {
        return Ok((attributes, String::new()));
    }
    let close = raw
        .rfind("</svg>")
        .ok_or_else(|| malformed("its <svg> element has no closing tag"))?;
    if close < open + after {
        return Err(malformed("its </svg> comes before its <svg>"));
    }
    Ok((attributes, raw[open + after + 1..close].trim().to_owned()))
}

/// The `viewBox`, or one derived from `width` and `height`.
fn view_box(attributes: &str) -> Option<String> {
    if let Some(declared) = attribute(attributes, "viewBox") {
        return Some(declared);
    }
    let width = attribute(attributes, "width")?;
    let height = attribute(attributes, "height")?;
    let number = |value: &str| value.trim_end_matches("px").trim().to_owned();
    Some(format!("0 0 {} {}", number(&width), number(&height)))
}

/// One attribute's value out of a tag's attribute text.
fn attribute(attributes: &str, name: &str) -> Option<String> {
    let mut from = 0;
    while let Some(at) = attributes[from..].find(name) {
        let start = from + at;
        let before_ok = start == 0 || attributes.as_bytes()[start - 1].is_ascii_whitespace();
        let rest = attributes[start + name.len()..].trim_start();
        if before_ok && let Some(value) = rest.strip_prefix('=') {
            let value = value.trim_start();
            let quote = value.chars().next()?;
            if quote == '"' || quote == '\'' {
                let end = value[1..].find(quote)?;
                return Some(value[1..1 + end].to_owned());
            }
        }
        from = start + name.len();
    }
    None
}

/// The width and height a `viewBox` declares.
fn dimensions(view_box: &str) -> Option<(f32, f32)> {
    let numbers: Vec<f32> = view_box
        .split([' ', ','])
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<f32>().ok())
        .collect();
    match numbers.as_slice() {
        [_, _, width, height] => Some((*width, *height)),
        _ => None,
    }
}

/// Prefix every `id` in the icon, and every reference to one, with `symbol`.
///
/// Two icons defining `<linearGradient id="a">` is the normal case, and in one
/// document the second definition wins for both. Textual rather than
/// tree-shaped for the same reason [`refuse_dangerous`] is: it has to agree
/// with what a browser resolves, not with a parser.
fn namespace_ids(inner: &str, symbol: &str) -> String {
    let mut out = inner.to_owned();
    for (needle, opener) in [("id=\"", "\""), ("id='", "'")] {
        out = rewrite(&out, needle, opener, symbol);
    }
    for (needle, opener) in [
        ("href=\"#", "\""),
        ("href='#", "'"),
        ("url(#", ")"),
        ("url(&quot;#", "&"),
    ] {
        out = rewrite(&out, needle, opener, symbol);
    }
    out
}

/// Insert `symbol-` after every `needle`, up to `closer`.
fn rewrite(text: &str, needle: &str, closer: &str, symbol: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(needle) {
        out.push_str(&rest[..at + needle.len()]);
        rest = &rest[at + needle.len()..];
        let Some(end) = rest.find(closer) else {
            break;
        };
        out.push_str(&format!("{symbol}-{}", &rest[..end]));
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// The part of a name that is safe in an id and in a URL fragment.
fn sanitise(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => character,
            _ => '-',
        })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        String::from("icon")
    } else {
        trimmed.chars().take(48).collect()
    }
}

/// Presentation attributes carried from the `<svg>` root onto the `<symbol>`.
///
/// An icon set puts its whole visual contract here — `fill="none"`,
/// `stroke="currentColor"`, `stroke-width="2"` — and every child inherits it.
/// Taking only the children and dropping the root would render a stroked icon
/// as a filled black blob, which is the kind of "it works" that is discovered
/// visually and never by a test.
///
/// An allow-list rather than everything: `class`, `id`, `style` and the
/// namespace declarations belong to the file and not to the shape, and copying
/// them into a shared sprite is how two icons start affecting each other.
const CARRIED: &[&str] = &[
    "fill",
    "fill-rule",
    "fill-opacity",
    "stroke",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-opacity",
    "clip-rule",
    "color",
    "opacity",
    "vector-effect",
    "preserveAspectRatio",
];

/// The carried attributes of one root, ready to append inside a tag.
fn carried(attributes: &str) -> String {
    let mut out = String::new();
    for name in CARRIED {
        if let Some(value) = attribute(attributes, name) {
            out.push_str(&format!(" {name}=\"{}\"", escape(&value)));
        }
    }
    out
}

/// The three characters that cannot appear in an XML attribute value.
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}
