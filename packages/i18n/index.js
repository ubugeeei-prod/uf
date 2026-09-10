// @flow
//
// `@uniflowed/i18n`: a message's arguments have a type.
//
// ```js
// import { defineCatalogue, message, number, string } from "@uniflowed/i18n";
//
// const messages = {
//   greeting: message("Hello, {$name}!", { name: string }),
//   unread: message(
//     `.input {$count :number}
// .match $count
// one {{You have {$count} unread message.}}
// *   {{You have {$count} unread messages.}}`,
//     { count: number },
//   ),
// };
//
// const en = defineCatalogue("en-US", messages);
//
// en.t("greeting", { name: "Ada" });   // "Hello, Ada!"
// en.t("unread", { count: 1 });        // "You have 1 unread message."
// en.t("unread", { count: 1200 });     // "You have 1,200 unread messages."
//
// en.t("unread", {});                  // Flow: property count is missing
// en.t("unread", { count: "12" });     // Flow: string is incompatible with number
// en.t("unreadd", { count: 1 });       // Flow: not a key of the catalogue
// ```
//
// The last three lines are the package. Every i18n library types the key;
// almost none types the arguments, and the one that does not is where the bugs
// are — a message that gains a `{$count}` in the source locale keeps compiling
// at every call site and renders the placeholder to a user.
//
// Ordinary Flow-typed JavaScript with no native binding and no dependencies,
// so it behaves identically on Node.js, Deno, Bun and in a browser.
//
// # Why MessageFormat 2 rather than a format of uf's own
//
// Because the hard parts of this are not syntax. Which of "1 message", "2
// messages" and "22 wiadomości" to use is CLDR's plural rules; where the
// number goes in a Japanese sentence is the translator's; whether `1200` is
// `1,200` or `1.200` is the locale's. A format invented here would have to
// answer all three eventually, and would answer them differently from every
// tool a translator already uses.
//
// MF2 is the Unicode standard for exactly this, it is final rather than
// proposed, and it is what translation tooling is moving to. Building on it
// means a message in this catalogue is a message a translation vendor can
// round-trip, and it means the pluralisation rules are CLDR's rather than
// somebody's guess.
//
// It also lands with typed placeholders, which is the part that makes it worth
// building *on* rather than around: `{$count :number}` says what `count` is,
// and that is the fact this package turns into a type error at the call.
//
// # The subset uf implements
//
// A partial implementation stated plainly, because the alternative is a
// complete one nobody can trust. Everything below is implemented and tested;
// everything under "not implemented" is refused at the point a message is
// declared, with an error naming itself, rather than accepted and quietly
// ignored.
//
// **Implemented.**
//
// - Simple messages, quoted patterns (`{{…}}`), and the four escapes
//   (`\\`, `\{`, `\}`, `\|`).
// - Variable placeholders `{$name}` and literal placeholders `{42}`, `{|two
//   words|}`.
// - The whole MF2 default function registry, and only it: `:string`,
//   `:number`, `:integer`, `:date`, `:time` and `:datetime`, with their
//   options, and with option values that may themselves be variables
//   (`{$n :number minimumFractionDigits=$digits}`).
// - `.input` and `.local` declarations, including an annotation put on a name
//   by a declaration and inherited by every later use of it.
// - `.match` with any number of selectors, literal and `*` variant keys, exact
//   numeric keys preferred over plural categories, and MF2's variant sort — so
//   a two-selector matcher resolves ties the way the specification says rather
//   than the way a single scan would.
//
// **Not implemented, deliberately.**
//
// - **Markup** — `{#bold}…{/bold}`. `t` returns a string, and markup only
//   means something to a caller that can turn a list of parts into React
//   elements. The two ways to fake it are both worse than refusing: dropping
//   the tags silently loses emphasis a translator put in, and inlining HTML
//   puts unescaped translator input into a page. A `parts` API returning
//   `$ReadOnlyArray<MessagePart>` is where this belongs, and it is a different
//   return type rather than a bigger parser.
// - **Attributes** — `{$x @unit}`. The specification says they do not affect
//   formatting, so accepting and ignoring them would be conforming. They are
//   refused because the only thing an attribute is for is a tool that reads
//   it, uf has no such tool, and a message carrying one would mean its author
//   believes something untrue.
// - **The draft function registry** — `:currency`, `:unit`, `:math`. The line
//   is drawn at "the whole required registry, none of the draft one" because
//   it is a line a reader can hold in their head: a function uf accepts is one
//   every conforming implementation must also accept. `:currency` is the one
//   that will be missed, and it needs a currency code in the message, which is
//   a decision about where currency codes live rather than a parser change.
// - **Reserved and private-use annotations** — `{$x !foo}`, `{$x ^bar}`.
//   Refusing them is what keeps a message that parses here from meaning
//   something else under a conforming implementation later.
// - **Bidi isolation is off by default**, which the specification allows
//   (`bidiIsolation: none`) but does not default to. `format.js` says why: the
//   output goes into React children where the DOM already isolates, and two
//   invisible code points per placeholder would make every string assertion in
//   every application a puzzle. `defineCatalogue(…, { bidiIsolation: true })`
//   turns it on.
//
// # Why uf formats MF2 itself, over `Intl`
//
// `Intl.MessageFormat` is a TC39 proposal with no implementation in any
// shipping runtime, so building on it means building on nothing. Feature-
// detecting it and falling back would be worse than not using it: two code
// paths deciding what a user reads, one of which has never run, and a
// catalogue that renders differently in Safari and in Node.
//
// So the algorithm is uf's and the data is `Intl`'s. Plural categories come
// from `Intl.PluralRules`, numerals from `Intl.NumberFormat`, dates from
// `Intl.DateTimeFormat`. The selection algorithm is a few hundred lines; CLDR's
// plural rules and number formats for the locales a browser already ships are
// megabytes, and a second, staler copy of them is exactly what a bundle does
// not need. When `Intl.MessageFormat` ships, the seam is one function in
// `format.js` rather than anything an application wrote.
//
// # Where the type system stops, and why
//
// Worth being exact about, because the promise above is a strong one and the
// limit is real.
//
// Flow has **no template-literal types**. The placeholders inside the string
// `"Hello, {$name}!"` are not part of that string's type, in Flow or in any
// checker without the TypeScript machinery — so `{ name: string }` cannot be
// *derived* from the message. It has to be written beside it.
//
// That is why parameters are declared as values rather than as a type
// argument, and it is not a workaround: because they are values, `message` can
// compare them with the message at run time, where it is written. A message
// that reads `$nom` when the parameters say `name`, a parameter the message
// never reads, an annotation that does not fit the declared kind — all three
// throw at the declaration, naming the key. A type argument would have been
// less to write and would have checked none of them, because a type argument
// is erased before anything could look at it.
//
// What is left unchecked is one thing, and it is worth saying rather than
// glossing: the *pair* is what is verified, so a message and its parameters
// that agree with each other and disagree with the sentence the product wanted
// are nobody's error. `t("unread", { count: 3 })` cannot render "3 messages"
// if the message says `{$count}` — but no tool here knows whether the product
// meant "unread" or "unarchived".
//
// A `uf lint` rule reading the type and the string literal together is what
// would close the remaining seam between a declaration and its call, and uf
// already parses Flow in Rust, so it is a rule rather than a research problem.
// It is not written.
//
// # How the package is laid out
//
// Four modules beside this one, each reachable through a subpath. Nothing is
// under an `internal/`: each is a reasonable thing to import on purpose, and a
// tool that wants only the parser should not carry the catalogue.
//
// - `syntax.js` — MF2 source into a tree, and the refusals that define the
//   subset. Knows nothing about locales or `Intl`. Read this first.
// - `format.js` — a tree, a locale and some arguments into a string:
//   declarations, the variant selection algorithm, and the `Intl` objects it
//   is all built on.
// - `catalogue.js` — the types that make a call checkable, the run-time check
//   that the message agrees with them, translations over the same keys, and
//   lazily loaded locales.
// - `negotiate.js` — `Accept-Language` into a list of tags, and RFC 4647
//   Lookup against the locales an application actually has.
//
// # A page ships one locale
//
// ```js
// const locales = defineLocales(en, {
//   ja: () => import("./ja.js").then((module) => module.default),
//   fr: () => import("./fr.js").then((module) => module.default),
// });
//
// const wanted = negotiate(
//   parseAcceptLanguage(request.headers.get("accept-language") ?? ""),
//   locales.available,
//   "en-US",
// );
// const t = (await locales.load(wanted)).t;
// ```
//
// The loaders are thunks so a bundler splits them: only the locale negotiation
// chose is fetched. A translation file is plain strings over the same keys —
// it does not redeclare the parameters, because those belong to the message
// rather than to the language — and `translate` holds each one against the
// source message's parameters at start-up, so a translator who drops a
// `{$count}` fails the build rather than the page.
//
// # Readiness
//
// **Implemented and tested.** Everything under "Implemented" above, the
// definition-time contract checks, translations with partial coverage and an
// `untranslated` list, lazily loaded locales with one load per locale, and
// negotiation over `Accept-Language` including quality values and `*`.
// `packages/i18n/i18n.test.js` covers each.
//
// **Not implemented, and a gap.** The catalogue is not extracted at build
// time. `uf build` does not walk a project for `message(…)` calls, so there is
// no `messages.json` for a translation vendor to import and no build-time
// report of a key that no locale translates. The half that belongs in this
// package — a parse complete enough to generate from, and `messageUsage` to
// read it — is here; the half that walks a repository's sources is Rust's, for
// the same reason the formatter and the checker are.
//
// **Not implemented, and declined.** A React hook and a provider. A catalogue
// is a value, `t` is a function on it, and a `useTranslation()` that read one
// out of context would put a re-render between a component and a string that
// does not change. An application that wants the locale in context already has
// `React.createContext`, and it should hold the catalogue rather than a hook
// this package invented.

export type {
  MessageAnnotation,
  MessageBody,
  MessageDeclaration,
  MessageExpression,
  MessageLiteral,
  MessageNode,
  MessageOperand,
  MessageOption,
  MessagePart,
  MessagePattern,
  MessageText,
  MessageUsage,
  MessageVariable,
  MessageVariant,
  MessageVariantKey,
} from "./syntax.js";
export { MessageSyntaxError, messageUsage, parseMessage } from "./syntax.js";

export type { FormatContext } from "./format.js";
export { MessageFormatError, formatMessage } from "./format.js";

export type {
  ArgsOf,
  Catalogue,
  CatalogueOptions,
  LocaleLoader,
  Locales,
  Message,
  MessageMap,
  Param,
  ParamArgs,
  ParamKind,
  ParamMap,
  ParamValue,
  Translations,
} from "./catalogue.js";
export {
  MessageContractError,
  boolean,
  date,
  defineCatalogue,
  defineLocales,
  message,
  number,
  string,
  translate,
} from "./catalogue.js";

export { negotiate, parseAcceptLanguage } from "./negotiate.js";
