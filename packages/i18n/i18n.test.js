// @flow
//
// `@uniflowed/i18n` under the runner that ships with the toolchain.
//
// The property worth testing here is not that `t` returns a string. It is that
// the three checks this package claims actually fire: Flow at the call site,
// the message-against-its-parameters check where a message is written, and the
// translation-against-its-source check at start-up. Two of those are run time
// and are asserted below; the first is Flow's, and the positive half of it is
// asserted here too — every `t` call in this file is annotated, so a `t` whose
// argument type stopped resolving would fail `uf check` rather than pass
// quietly as `any`.
//
// The rest is the MF2 subset, and the cases chosen are the ones where a
// plausible implementation is wrong: exact numeric keys against plural
// categories, a two-selector matcher's tie-break, an annotation inherited
// through `.input`, and every construct uf refuses.

import path from "node:path";

import { describe, expect, it } from "@uniflowed/test";
import type { Catalogue } from "@uniflowed/i18n";

import { everyMisuseIsReported } from "../../tests/library/type-tests.js";
import {
  MessageContractError,
  MessageFormatError,
  MessageSyntaxError,
  boolean,
  date,
  defineCatalogue,
  defineLocales,
  message,
  messageUsage,
  negotiate,
  number,
  parseAcceptLanguage,
  parseMessage,
  string,
  translate,
} from "@uniflowed/i18n";

/** The plural message every test that needs one uses. */
const UNREAD = `.input {$count :number}
.match $count
0   {{No unread messages.}}
one {{You have {$count} unread message.}}
*   {{You have {$count} unread messages.}}`;

const messages = {
  greeting: message("Hello, {$name}!", { name: string }),
  unread: message(UNREAD, { count: number }),
  cartEmpty: message("Your cart is empty.", {}),
};

function english(): Catalogue<typeof messages> {
  return defineCatalogue("en-US", messages);
}

describe("the promise: a message's arguments are typed", () => {
  it("formats a message with the arguments it was given", () => {
    const t = english().t;

    expect(t("greeting", { name: "Ada" })).toBe("Hello, Ada!");
    expect(t("cartEmpty", {})).toBe("Your cart is empty.");
  });

  /**
   * The typing has to survive being read through the catalogue's type rather
   * than only at the definition site.
   *
   * `Catalogue<typeof messages>` is what an application writes when it puts a
   * catalogue in a module of its own, and `ArgsOf<TMessages[TKey]>` has to
   * resolve through that annotation for the package to be usable that way. An
   * annotated binding is how this file asserts it: if the argument type
   * degraded to `any`, `uf check` would still pass here, but the `number`
   * below would not be a `number`.
   */
  it("carries the argument type through an annotated catalogue", () => {
    const catalogue: Catalogue<typeof messages> = english();
    const greeting: string = catalogue.t("greeting", { name: "Ada" });
    const count: number = 3;

    expect(greeting).toBe("Hello, Ada!");
    expect(catalogue.t("unread", { count })).toBe("You have 3 unread messages.");
  });

  /**
   * A key that is not in the catalogue is a Flow error, and a key that arrived
   * as a plain string at run time has to fail loudly rather than render
   * nothing.
   *
   * The second case is the one worth a test: a key read out of a database, a
   * URL segment or a CMS is a `string`, Flow never saw it, and returning the
   * empty string for it would put a silently blank label on a page.
   */
  it("raises for a key that is not in the catalogue", () => {
    const catalogue = english();

    expect(() => catalogue.sourceOf("nonesuch")).toThrow(MessageContractError);
    expect(() => catalogue.sourceOf("nonesuch")).toThrow("is not in this catalogue");
  });

  it("hands back the MF2 source behind a key", () => {
    expect(english().sourceOf("greeting")).toBe("Hello, {$name}!");
  });
});

describe("the contract between a message and its parameters", () => {
  /**
   * The check that exists because Flow cannot do it.
   *
   * Flow has no template-literal types, so `{ name: string }` cannot be
   * derived from `"Hello, {$nom}!"` and cannot be checked against it either.
   * Declaring the parameters as values is what makes this comparison possible
   * at all, and this test is the reason the API is shaped that way rather than
   * taking a type argument.
   */
  it("refuses a message that reads a parameter nobody declared", () => {
    expect(() => message("Hello, {$nom}!", { name: string })).toThrow(MessageContractError);
    expect(() => message("Hello, {$nom}!", { name: string })).toThrow("reads $nom");
  });

  /**
   * The other direction, which is the half of a rename that lands in one place.
   *
   * Tolerating an unused parameter would mean every call site keeps passing a
   * value that reaches nothing, which is invisible forever.
   */
  it("refuses a parameter the message never reads", () => {
    expect(() => message("Hello!", { name: string })).toThrow("declares $name");
  });

  /**
   * A kind mismatch is not a type error anywhere: the parameter's type and the
   * message's annotation are two facts about one name that only meet here.
   */
  it("refuses an annotation that does not fit the declared kind", () => {
    expect(() => message("Sorted by {$at :date}", { at: number })).toThrow(
      "applies :date to $at, which is declared number",
    );
    expect(() => message("{$n :number}", { n: date })).toThrow("declared date");
  });

  it("accepts every annotation a kind does allow", () => {
    expect(() => message("{$n :integer}", { n: number })).not.toThrow();
    expect(() => message("{$at :time}", { at: date })).not.toThrow();
    expect(() => message("{$at :datetime}", { at: date })).not.toThrow();
    expect(() => message("{$who :string}", { who: string })).not.toThrow();
    expect(() => message(".match $on\ntrue {{yes}}\n* {{no}}", { on: boolean })).not.toThrow();
  });

  /**
   * A `.local` introduces a name that is not a parameter, and an `.input`
   * re-annotates one that is.
   *
   * Getting this backwards makes `.local` unusable — every message with one
   * would demand an argument nobody has — and it is the kind of thing that
   * only shows up on the first message complicated enough to need one.
   */
  it("counts what a declaration introduces as a parameter only when it is one", () => {
    expect(() =>
      message(".local $shown = {$count :number}\n{{{$shown} left}}", { count: number }),
    ).not.toThrow();
    expect(() =>
      message(".local $shown = {$count :number}\n{{{$shown} left}}", {
        count: number,
        shown: number,
      }),
    ).toThrow("declares $shown");
  });

  it("refuses a function that is not in the MF2 default registry", () => {
    expect(() => message("{$price :currency currency=EUR}", { price: number })).toThrow(
      "names the function :currency",
    );
  });
});

describe("the MF2 subset uf refuses", () => {
  /**
   * Markup is refused rather than dropped.
   *
   * Dropping `{#bold}` would silently lose emphasis a translator put in, and a
   * message that formats to plausible-looking text with the markup gone is the
   * failure that reaches production. See the module header for why a string
   * return type is what forces the choice.
   */
  it("refuses markup, because a string cannot carry it", () => {
    expect(() => parseMessage("Click {#link}here{/link}")).toThrow(MessageSyntaxError);
    expect(() => parseMessage("Click {#link}here{/link}")).toThrow("markup");
  });

  it("refuses an attribute, which nothing in uf would read", () => {
    expect(() => parseMessage("{$x :string @unit}")).toThrow("@attribute");
  });

  it("refuses an annotation MessageFormat 2 reserves", () => {
    expect(() => parseMessage("{$x !reserved}")).toThrow("reserves");
    expect(() => parseMessage("{$x ^private}")).toThrow("reserves");
  });

  it("refuses a namespaced function", () => {
    expect(() => parseMessage("{$x :acme:money}")).toThrow("namespaced function");
  });

  /**
   * A `.match` with no `*` formats correctly for every value somebody tried
   * and throws on the first one they did not.
   *
   * Caught where the message is written rather than where it is formatted,
   * which is the difference between a start-up error and a page that breaks
   * for one locale at one number.
   */
  it("refuses a matcher with no catch-all variant", () => {
    expect(() => parseMessage(".match $n\none {{one}}")).toThrow("all *");
  });

  it("refuses a duplicate declaration", () => {
    expect(() => parseMessage(".local $a = {1}\n.local $a = {2}\n{{x}}")).toThrow(
      "$a is declared twice",
    );
  });
});

describe("the parser", () => {
  it("reads a simple message as text and placeholders", () => {
    const node = parseMessage("Hello, {$name}!");

    expect(node.declarations).toEqual([]);
    expect(node.body.kind).toBe("pattern");
    expect(messageUsage(node).variables).toEqual(["name"]);
  });

  /**
   * MF2 decides simple against complex on the first character alone, so a
   * leading space turns a matcher into ordinary text.
   *
   * Trimming would be a kindness that changed what a message means, and this
   * records the decision rather than only the behaviour.
   */
  it("treats a message that does not start with a dot as text", () => {
    expect(parseMessage(".match $n\n* {{many}}").body.kind).toBe("select");
    expect(parseMessage(" .match $n\n* many").body.kind).toBe("pattern");
  });

  it("resolves the four escapes and refuses anything else after a backslash", () => {
    const catalogue = defineCatalogue("en-US", {
      braces: message("a \\{ b \\} c \\\\ d \\| e", {}),
    });

    expect(catalogue.t("braces", {})).toBe("a { b } c \\ d | e");
    expect(() => parseMessage("line\\nbreak")).toThrow("backslash");
  });

  it("refuses an unescaped closing brace in a pattern", () => {
    expect(() => parseMessage("closing } brace")).toThrow("unescaped }");
  });

  it("refuses a quoted literal that is never closed", () => {
    expect(() => parseMessage("{|never")).toThrow("never closed");
  });

  it("reads an option value that is itself a variable", () => {
    expect(
      messageUsage(parseMessage("{$n :number minimumFractionDigits=$places}")).variables,
    ).toEqual(["n", "places"]);
  });

  it("reads the functions a message names", () => {
    expect(messageUsage(parseMessage("{$a :number} {$b :date}")).functions).toEqual([
      "date",
      "number",
    ]);
  });
});

describe("formatting over Intl", () => {
  it("formats a number in the catalogue's locale", () => {
    const en = defineCatalogue("en-US", { total: message("{$n :number}", { n: number }) });
    const de = defineCatalogue("de-DE", { total: message("{$n :number}", { n: number }) });

    expect(en.t("total", { n: 1234567.5 })).toBe("1,234,567.5");
    expect(de.t("total", { n: 1234567.5 })).toBe("1.234.567,5");
  });

  it("passes the MF2 number options through to Intl", () => {
    const catalogue = defineCatalogue("en-US", {
      share: message("{$n :number style=percent}", { n: number }),
      fixed: message("{$n :number minimumFractionDigits=2}", { n: number }),
      whole: message("{$n :integer}", { n: number }),
    });

    expect(catalogue.t("share", { n: 0.25 })).toBe("25%");
    expect(catalogue.t("fixed", { n: 1 })).toBe("1.00");
    expect(catalogue.t("whole", { n: 2.7 })).toBe("3");
  });

  /**
   * A bare `{$count}` with a number in it formats as a number.
   *
   * MF2 leaves an unannotated placeholder to the implementation, and the other
   * choice — everything is a string — renders 1234567 identically in every
   * locale, which is the bug this package exists to prevent.
   */
  it("formats an unannotated placeholder by the type of its value", () => {
    const catalogue = defineCatalogue("en-US", {
      plain: message("{$n}", { n: number }),
      when: message("{$at}", { at: date }),
    });

    expect(catalogue.t("plain", { n: 1234567 })).toBe("1,234,567");
    expect(catalogue.t("when", { at: new Date(0) })).toContain("1970");
  });

  it("formats a date with the style it was given", () => {
    const catalogue = defineCatalogue("en-GB", {
      day: message("{$at :date style=short timeZone=UTC}", { at: date }),
    });

    expect(catalogue.t("day", { at: new Date("2026-09-07T12:00:00Z") })).toBe("07/09/2026");
  });

  /**
   * An annotation put on a name by `.input` travels with the name.
   *
   * This is the only reason `.input` is worth having over repeating the
   * annotation at every use, and a resolver that forgot it would format the
   * first `{$count}` as a number and the second as a bare string — in the same
   * sentence.
   */
  it("carries an annotation from a declaration into every later use", () => {
    const catalogue = defineCatalogue("en-US", {
      both: message(".input {$n :number}\n{{{$n} and {$n}}}", { n: number }),
    });

    expect(catalogue.t("both", { n: 1234 })).toBe("1,234 and 1,234");
  });

  it("formats a literal placeholder", () => {
    const catalogue = defineCatalogue("en-US", {
      quoted: message("a {|two words|} b {42 :integer} c", {}),
    });

    expect(catalogue.t("quoted", {})).toBe("a two words b 42 c");
  });

  /**
   * Bidi isolation is off by default and the specification defaults it on.
   *
   * Both halves are asserted so that the departure stays a decision: the day
   * somebody flips the default, one of these two fails and they have to read
   * why.
   */
  it("isolates placeholders only when asked to", () => {
    const plain = defineCatalogue("en-US", { hi: message("[{$who}]", { who: string }) });
    const isolated = defineCatalogue(
      "en-US",
      { hi: message("[{$who}]", { who: string }) },
      { bidiIsolation: true },
    );

    expect(plain.t("hi", { who: "x" })).toBe("[x]");
    expect(isolated.t("hi", { who: "x" })).toBe("[⁨x⁩]");
  });
});

describe("selection", () => {
  it("chooses a variant by the locale's plural category", () => {
    const catalogue = english();

    expect(catalogue.t("unread", { count: 1 })).toBe("You have 1 unread message.");
    expect(catalogue.t("unread", { count: 5 })).toBe("You have 5 unread messages.");
  });

  /**
   * An exact numeric key beats the plural category it also falls into.
   *
   * `0` is `other` in English, so a selector that only consulted
   * `Intl.PluralRules` would render "You have 0 unread messages." and the `0`
   * variant would be unreachable — which is exactly the special case a message
   * author reaches for a literal key to express.
   */
  it("prefers an exact numeric key over the plural category", () => {
    expect(english().t("unread", { count: 0 })).toBe("No unread messages.");
  });

  it("uses the locale's own plural categories, not English's", () => {
    const polish = defineCatalogue("pl-PL", {
      files: message(
        `.match $n
one  {{{$n} plik}}
few  {{{$n} pliki}}
many {{{$n} plikow}}
*    {{{$n} pliku}}`,
        { n: number },
      ),
    });

    expect(polish.t("files", { n: 1 })).toBe("1 plik");
    expect(polish.t("files", { n: 3 })).toBe("3 pliki");
    expect(polish.t("files", { n: 5 })).toBe("5 plikow");
  });

  it("selects on ordinal categories when asked", () => {
    const catalogue = defineCatalogue("en-US", {
      place: message(
        `.input {$n :number select=ordinal}
.match $n
one {{{$n}st}}
two {{{$n}nd}}
few {{{$n}rd}}
*   {{{$n}th}}`,
        { n: number },
      ),
    });

    expect(catalogue.t("place", { n: 1 })).toBe("1st");
    expect(catalogue.t("place", { n: 2 })).toBe("2nd");
    expect(catalogue.t("place", { n: 3 })).toBe("3rd");
    expect(catalogue.t("place", { n: 4 })).toBe("4th");
  });

  it("selects on an exact string", () => {
    const catalogue = defineCatalogue("en-US", {
      role: message(
        `.match $role
admin  {{You can edit anything.}}
editor {{You can edit your own posts.}}
*      {{You can read.}}`,
        { role: string },
      ),
    });

    expect(catalogue.t("role", { role: "admin" })).toBe("You can edit anything.");
    expect(catalogue.t("role", { role: "guest" })).toBe("You can read.");
  });

  /**
   * With two selectors, "best" is not a property of one row on its own.
   *
   * The later selector breaks ties within the earlier one, so `one female` has
   * to beat `one *`, which has to beat `* female`. A single scan for the row
   * with the most literal keys gets the second pair backwards, and the bug
   * only appears in messages with two selectors — which are the ones nobody
   * writes a test for.
   */
  it("breaks a tie between two selectors the way MF2 says", () => {
    const catalogue = defineCatalogue("en-US", {
      invite: message(
        `.input {$count :number}
.match $count $gender
one  female {{She invited one guest.}}
one  *      {{They invited one guest.}}
*    female {{She invited {$count} guests.}}
*    *      {{They invited {$count} guests.}}`,
        { count: number, gender: string },
      ),
    });
    const t = catalogue.t;

    expect(t("invite", { count: 1, gender: "female" })).toBe("She invited one guest.");
    expect(t("invite", { count: 1, gender: "male" })).toBe("They invited one guest.");
    expect(t("invite", { count: 4, gender: "female" })).toBe("She invited 4 guests.");
    expect(t("invite", { count: 4, gender: "male" })).toBe("They invited 4 guests.");
  });

  it("refuses a date as a selector", () => {
    const catalogue = defineCatalogue("en-US", {
      when: message(".input {$at :date}\n.match $at\n* {{whenever}}", { at: date }),
    });

    expect(() => catalogue.t("when", { at: new Date(0) })).toThrow("cannot be a .match selector");
  });
});

describe("what happens when a value bypassed the type system", () => {
  /**
   * `onError` throws by default because reaching it means the types were
   * bypassed — a JSON response, a server prop, an `any`.
   *
   * The `$FlowFixMe` below is the point of the test rather than a shortcut: it
   * is standing in for the value that arrived from outside, and it is the only
   * way to reach this path at all.
   */
  it("throws by default for a value the message cannot format", () => {
    const catalogue = defineCatalogue("en-US", { n: message("{$n :number}", { n: number }) });
    const fromOutside = { n: "many" } as $FlowFixMe;

    expect(() => catalogue.t("n", fromOutside)).toThrow(MessageFormatError);
    expect(() => catalogue.t("n", fromOutside)).toThrow("is not a number");
  });

  /**
   * An application that would rather show the MF2 fallback than fail a render
   * says so, and gets the specification's fallback text.
   */
  it("emits the MF2 fallback when onError is told to record instead", () => {
    const seen: Array<string> = [];
    const catalogue = defineCatalogue(
      "en-US",
      { n: message("total: {$n :number}", { n: number }) },
      {
        onError: (error) => {
          seen.push(error.fallback);
        },
      },
    );

    expect(catalogue.t("n", { n: "many" } as $FlowFixMe)).toBe("total: {$n}");
    expect(seen).toEqual(["{$n}"]);
  });
});

describe("translations", () => {
  it("formats a translated message and reports what was not translated", () => {
    const ja = translate(english(), "ja-JP", { greeting: "こんにちは、{$name}さん!" });

    expect(ja.t("greeting", { name: "アダ" })).toBe("こんにちは、アダさん!");
    expect(ja.locale).toBe("ja-JP");
    expect(ja.untranslated).toEqual(["unread", "cartEmpty"]);
  });

  it("falls back to the source message for a key the locale has not translated", () => {
    const ja = translate(english(), "ja-JP", { greeting: "こんにちは、{$name}さん!" });

    expect(ja.t("cartEmpty", {})).toBe("Your cart is empty.");
  });

  /**
   * A translator who drops a placeholder fails at start-up, naming the key.
   *
   * This is the check that makes a partial translation safe to ship: a missing
   * key falls back and is reported, and a *wrong* key does not get to fall
   * back — it would render a sentence with the number missing.
   */
  it("refuses a translation that lost a placeholder", () => {
    expect(() => translate(english(), "fr-FR", { greeting: "Bonjour !" })).toThrow(
      MessageContractError,
    );
    expect(() => translate(english(), "fr-FR", { greeting: "Bonjour !" })).toThrow("greeting");
  });

  it("refuses a translation that invented a placeholder", () => {
    expect(() => translate(english(), "fr-FR", { greeting: "Bonjour {$nom} !" })).toThrow(
      "reads $nom",
    );
  });

  it("refuses a translation key the source catalogue does not have", () => {
    const misspelled = { greetings: "Bonjour !" } as $FlowFixMe;

    expect(() => translate(english(), "fr-FR", misspelled)).toThrow(
      "is not in the source catalogue",
    );
  });

  /**
   * A language may need a different set of variants from the source.
   *
   * The check is on the *parameters*, not on the shape of the message, so a
   * language with more plural categories than English is allowed to use them.
   */
  it("allows a translation to change the variants as long as the parameters hold", () => {
    const ru = translate(english(), "ru-RU", {
      unread: `.input {$count :number}
.match $count
one  {{{$count} непрочитанное сообщение}}
few  {{{$count} непрочитанных сообщения}}
*    {{{$count} непрочитанных сообщений}}`,
    });

    expect(ru.t("unread", { count: 1 })).toBe("1 непрочитанное сообщение");
    expect(ru.t("unread", { count: 3 })).toBe("3 непрочитанных сообщения");
    expect(ru.t("unread", { count: 7 })).toBe("7 непрочитанных сообщений");
  });

  /**
   * A translated catalogue formats numbers in its own locale, not the source's.
   *
   * The catalogue carries the locale into `Intl`, and a translation that
   * inherited the source's would print German text with English digit
   * grouping.
   */
  it("formats a translated catalogue's numbers in the translated locale", () => {
    const de = translate(english(), "de-DE", {
      unread: ".input {$count :number}\n.match $count\n* {{{$count} ungelesen}}",
    });

    expect(de.t("unread", { count: 1234 })).toBe("1.234 ungelesen");
  });
});

describe("lazily loaded locales", () => {
  it("resolves the source locale without loading anything", async () => {
    let loads = 0;
    const locales = defineLocales(english(), {
      ja: () => {
        loads += 1;
        return Promise.resolve({ greeting: "こんにちは、{$name}さん!" });
      },
    });

    expect(locales.available).toEqual(["en-US", "ja"]);
    expect((await locales.load("en-US")).locale).toBe("en-US");
    expect(loads).toBe(0);
  });

  /**
   * Two concurrent requests for one locale share one load.
   *
   * A server handling a burst of Japanese requests must not fetch the
   * translation chunk once per request, and caching the *promise* rather than
   * the catalogue is what makes the second request wait for the first instead
   * of starting a second download.
   */
  it("loads a locale at most once, even for concurrent callers", async () => {
    let loads = 0;
    const locales = defineLocales(english(), {
      ja: () => {
        loads += 1;
        return Promise.resolve({ greeting: "こんにちは、{$name}さん!" });
      },
    });

    const [first, second] = await Promise.all([locales.load("ja"), locales.load("ja")]);

    expect(loads).toBe(1);
    expect(first).toBe(second);
    expect(first.t("greeting", { name: "アダ" })).toBe("こんにちは、アダさん!");
  });

  it("rejects a locale the application does not have", async () => {
    const locales = defineLocales(english(), {});

    await expect(locales.load("ja")).rejects.toThrow("is not a locale this application has");
  });
});

describe("negotiation", () => {
  it("orders an Accept-Language header by quality and then by position", () => {
    expect(parseAcceptLanguage("en-GB,en;q=0.9,fr;q=0.8,*;q=0.5")).toEqual([
      "en-GB",
      "en",
      "fr",
      "*",
    ]);
    expect(parseAcceptLanguage("en, fr")).toEqual(["en", "fr"]);
  });

  /**
   * `q=0` means "not acceptable", not "last".
   *
   * Treating it as a low weight would pick a language the reader explicitly
   * refused, which is the one outcome the header exists to prevent.
   */
  it("drops a language the reader refused with q=0", () => {
    expect(parseAcceptLanguage("de;q=0, en")).toEqual(["en"]);
  });

  it("ignores an empty or malformed header", () => {
    expect(parseAcceptLanguage("")).toEqual([]);
    expect(parseAcceptLanguage("en;q=nonsense")).toEqual(["en"]);
  });

  it("takes the first requested tag it has", () => {
    expect(negotiate(["fr-CA", "en"], ["en-US", "fr", "ja"], "en-US")).toBe("fr");
  });

  /**
   * RFC 4647 Lookup truncates the request one subtag at a time, and skips a
   * single-character subtag rather than trying it.
   *
   * `zh-x` is not a language tag: a one-character subtag starts a private-use
   * sequence, so truncating to it leaves a prefix that can only match by
   * accident.
   */
  it("truncates the requested tag, skipping a single-character subtag", () => {
    expect(negotiate(["en-Latn-GB-oed"], ["en"], "ja")).toBe("en");
    expect(negotiate(["zh-x-private"], ["zh"], "en")).toBe("zh");
  });

  it("returns the available tag with its own spelling, not the folded one", () => {
    expect(negotiate(["en-us"], ["en-US"], "ja")).toBe("en-US");
  });

  /**
   * The documented departure from RFC 4647, and the reason it runs last.
   *
   * Lookup only shortens the request, so a reader asking for `en` does not
   * match a catalogue that only has `en-US` — and the alternative for that
   * reader is not British English, it is whatever the fallback locale happens
   * to be. The pass runs after every real match so that it can never outrank
   * one.
   */
  it("falls back to a shared primary subtag, but only after every real match", () => {
    expect(negotiate(["en"], ["ja", "en-US"], "ja")).toBe("en-US");
    expect(negotiate(["en-GB", "fr"], ["fr-FR", "en-AU"], "ja")).toBe("en-AU");
  });

  it("returns the fallback when it has none of the requested languages", () => {
    expect(negotiate(["ko", "th"], ["en-US", "fr"], "en-US")).toBe("en-US");
    expect(negotiate("*", ["en-US", "fr"], "fr")).toBe("fr");
    expect(negotiate([], ["en-US"], "en-US")).toBe("en-US");
  });

  it("accepts a single tag as well as a list", () => {
    expect(negotiate("fr-CA", ["en-US", "fr"], "en-US")).toBe("fr");
  });
});

describe("the five misuses a message's type has to refuse", () => {
  // The claim `@uniflowed/i18n` is for, and the one nothing in this file can
  // hold it to: `t("unread", {})`, `t("unread", { count: "12" })`,
  // `t("unreadd", …)`, an argument too many and an argument to a message that
  // takes none are all *type errors at the call*. No assertion about behaviour
  // can say a different program would have been rejected — `t("unread", {})`
  // throws at run time, but so would a `t` with no types at all — so it is
  // proved the only way it can be, by running the checker over code that must
  // fail and reading what it said.
  //
  // All five were verified against a scratch file while #567 was written, and
  // the file was thrown away. `tests/type-tests/i18n.js` is that file, kept:
  // the five misuses beside the calls that must keep compiling, marked with
  // `// expect:` comments the harness compares against `uf check`.

  it("reports every misuse, and only the misuses", () => {
    everyMisuseIsReported({
      fixture: path.join("tests", "type-tests", "i18n.js"),
      alongside: ["packages/i18n"],
      atLeast: 4,
    });
  });
});
