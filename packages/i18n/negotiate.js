// @flow
//
// `@uniflowed/i18n/negotiate`: which of the locales we have does this reader
// want.
//
// Two functions and no state. Nothing here loads anything, and that is the
// point of it being separate: a server decides the locale before it decides
// what to render, from a header and a list of tags, and both of those are
// known long before a catalogue exists. `defineLocales` in `catalogue.js` is
// what turns the answer into messages.
//
// # Lookup, not filtering
//
// RFC 4647 defines two matching schemes and they answer different questions.
// *Filtering* returns every tag that matches, which is what a search does.
// *Lookup* returns the single best one, which is what a page does, because a
// page renders in one language. So this is Lookup: the requested tag is
// truncated one subtag at a time — `en-Latn-GB-oed`, `en-Latn-GB`, `en-Latn`,
// `en` — and the first truncation that names something we have wins.
//
// The single-character subtag is skipped rather than tried, because RFC 4647
// says so and the reason is worth knowing: `zh-x-private` truncated to `zh-x`
// is not a language tag at all, it is the start of a private-use sequence with
// nothing after it.
//
// # The one deliberate departure
//
// Lookup only ever shortens the *request*, so a reader asking for `en` does
// not match a catalogue that has `en-US`. That is correct for the internet RFC
// 4647 was written for, where a server could generate a document for any tag
// it was asked about. It is wrong for an application with a fixed set of
// translation files, where the alternative to American English for a reader
// who asked for English is not British English — it is the default locale,
// which may be Japanese.
//
// So there is a final pass, after every exact and truncated match has failed
// for every requested tag: match on the primary language subtag alone, taking
// the first available tag that shares it. It runs last, so it can never take
// priority over a real match, and it is the difference between "we have your
// language" and "we have your language and refused to use it".
//
// # Why `Intl.LocaleMatcher` is not used
//
// Because it does not exist. `Intl.supportedValuesOf` and
// `Intl.getCanonicalLocales` are not a matcher, and the locale negotiation
// inside `Intl.NumberFormat` is not reachable from outside it — passing a
// list and reading `resolvedOptions().locale` back answers which locale *ICU*
// has data for, which is nearly every locale, and says nothing about which
// ones this application has translations for.

/** One entry of an `Accept-Language` header. */
type Weighted = { readonly tag: string, readonly quality: number, readonly at: number };

/**
 * `Accept-Language` as a list of tags, best first.
 *
 * Entries with `q=0` are dropped: RFC 9110 gives that the specific meaning
 * "not acceptable", so treating it as merely last would pick a language the
 * reader explicitly refused.
 *
 * `*` is kept, as the tag `*`. It means "anything", and the only sensible
 * answer to it is the fallback, which is what [`negotiate`] already returns
 * when nothing matches — so it needs no special case there, only here, where
 * dropping it would be wrong for a header that is nothing but `*`.
 */
export function parseAcceptLanguage(header: string): $ReadOnlyArray<string> {
  const entries: Array<Weighted> = [];

  header.split(",").forEach((part, index) => {
    const pieces = part.split(";");
    const tag = pieces[0].trim();
    if (tag === "") return;

    let quality = 1;
    for (const parameter of pieces.slice(1)) {
      const [name, value] = parameter.split("=");
      if (name.trim().toLowerCase() !== "q") continue;
      const parsed = Number(value);
      quality = Number.isFinite(parsed) ? parsed : 1;
    }
    if (quality <= 0) return;
    entries.push({ tag, quality, at: index });
  });

  // Sorted by quality, and by position within one quality. A header listing
  // `en, fr` without weights means the reader prefers English, and a sort that
  // only looked at the number would be free to return them the other way
  // round.
  entries.sort((left, right) =>
    left.quality === right.quality ? left.at - right.at : right.quality - left.quality,
  );
  return entries.map((entry) => entry.tag);
}

/** Language tags are case-insensitive, and nothing else about them matters here. */
function fold(tag: string): string {
  return tag.toLowerCase();
}

/** The truncations of a tag, longest first, per RFC 4647's Lookup. */
function truncations(tag: string): $ReadOnlyArray<string> {
  const out: Array<string> = [];
  let current = fold(tag);
  while (current !== "") {
    out.push(current);
    const cut = current.lastIndexOf("-");
    if (cut < 0) break;
    current = current.slice(0, cut);
    // `en-x` and `zh-a` are not tags: a single-character subtag introduces an
    // extension or a private-use sequence, so truncating to it leaves a prefix
    // with nothing to match.
    const last = current.lastIndexOf("-");
    if (current.length - last === 2) {
      current = current.slice(0, last);
    }
  }
  return out;
}

function primary(tag: string): string {
  const cut = fold(tag).indexOf("-");
  return cut < 0 ? fold(tag) : fold(tag).slice(0, cut);
}

/**
 * The best of `available` for a reader who asked for `requested`.
 *
 * `requested` is in preference order — what [`parseAcceptLanguage`] returns, or
 * a single tag from a cookie or a URL segment. `available` is what the
 * application has, and `fallback` is what it does when it has none of them.
 *
 * The returned tag is one of `available` verbatim, case and all, rather than
 * the folded form matching used — a caller is going to hand it to
 * `defineLocales`, which keys on the string the application wrote.
 */
export function negotiate(
  requested: $ReadOnlyArray<string> | string,
  available: $ReadOnlyArray<string>,
  fallback: string,
): string {
  const wanted = typeof requested === "string" ? [requested] : requested;
  const folded = available.map(fold);

  for (const tag of wanted) {
    if (tag === "*") break;
    for (const candidate of truncations(tag)) {
      const found = folded.indexOf(candidate);
      if (found >= 0) return available[found];
    }
  }

  // The departure from RFC 4647, and last so that it cannot outrank a real
  // match. See the module header.
  for (const tag of wanted) {
    if (tag === "*") break;
    const language = primary(tag);
    const found = folded.findIndex((candidate) => primary(candidate) === language);
    if (found >= 0) return available[found];
  }

  return fallback;
}
