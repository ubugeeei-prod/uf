// @flow
//
// The five misuses `@uniflowed/i18n` exists to refuse, and the calls it must
// keep accepting.
//
// This file is *supposed* to fail `uf check`. The whole design of the package
// rests on Flow catching five things at a `t` call — a missing argument, an
// argument of the wrong type, a misspelt key, an extra argument, and an
// argument handed to a message that takes none — and a claim like that is not
// provable by running anything: no assertion about behaviour can say that a
// *different* program would have been rejected. All five were checked by hand
// against a scratch file while #567 was written, and a scratch file survives no
// refactor. That is ubugeeei-prod/uf#568.
//
// The other half matters as much and is the half a scratch file never has: the
// calls at the bottom that must keep compiling. A `t` that refused everything
// would satisfy the five markers above it and take the library away.
//
// # How it is read
//
// A `// expect:` comment says that the line after it must be reported, and that
// the report must contain that text. A line without one must not be reported at
// all. `tests/library/i18n.test.js` runs `uf check` and compares the two, via
// the shared harness in `tests/library/type-tests.js`.
//
// # Why it is checked with the package rather than on its own
//
// `uf check` builds its module map out of the files it is asked to check, and a
// relative import that leaves that set resolves to an any-typed value — after
// which `ArgsOf` is `any`, every line below passes, and the test proves the
// opposite of what it says. So it runs `uf check tests/type-tests
// packages/i18n`, with both in one set.
//
// # Why it is not inside the package
//
// A file of deliberate type errors inside `packages/i18n` would be shipped to
// anyone who installed it and would fail every check the package runs on
// itself. It lives here for the same reason `anchoring.js` and `field-paths.js`
// do.

import type { Catalogue } from "../../packages/i18n/index.js";
import { defineCatalogue, message, number, string } from "../../packages/i18n/index.js";

const UNREAD = `.input {$count :number}
.match $count
one {{You have {$count} unread message.}}
*   {{You have {$count} unread messages.}}`;

const messages = {
  greeting: message("Hello, {$name}!", { name: string }),
  unread: message(UNREAD, { count: number }),
  cartEmpty: message("Your cart is empty.", {}),
};

const en: Catalogue<typeof messages> = defineCatalogue("en-US", messages);

// --- the five ---------------------------------------------------------------
//
// One per line, each with the annotation on the result, so that a `t` whose
// return type stopped resolving would be reported here rather than passing as
// `any` on a line that was supposed to fail for a different reason.

// A missing argument. The message reads `$name` and nothing was passed, which
// is the ordinary shape of "a placeholder was added to the English and the call
// sites were not visited".

// expect: property name is missing in object literal but exists in ParamArgs
export const missingArgument: string = en.t("greeting", {});

// An argument of the wrong type. `count` is declared `number` beside a message
// that annotates it `:number`, and a string here would format as the literal
// text rather than through `Intl.NumberFormat`.

// expect: in property count: "12" is incompatible with number
export const wronglyTypedArgument: string = en.t("unread", { count: "12" });

// A misspelt key. Every i18n library catches this one; it is here because the
// generic `t` has to keep catching it while it also checks the arguments.

// expect: property unreadd (did you mean unread?) is missing
export const misspeltKey: string = en.t("unreadd", { count: 1 });

// An extra argument. Not a typo of nothing — it is a `{$count}` that a
// translator removed, or a parameter renamed in the message and not at the
// call, and a caller computing a value that reaches no output should hear so.

// expect: property count is extra in object literal but missing in ParamArgs
export const extraArgument: string = en.t("greeting", { name: "Ada", count: 1 });

// An argument to a message that takes none. `cartEmpty` is declared with `{}`,
// which is what makes `t("cartEmpty", {})` the required spelling — see the
// module header of `catalogue.js` for why the empty object is not an oversight
// — and this is the misuse that spelling has to be able to refuse.

// expect: property name is extra in object literal but missing in ParamArgs
export const argumentToAMessageWithNone: string = en.t("cartEmpty", { name: "Ada" });

// --- and the calls that must keep compiling ---------------------------------
//
// The half that says the type is a set of narrow refusals rather than a `t`
// nothing satisfies. Nothing below this line may be reported.

export const greeting: string = en.t("greeting", { name: "Ada" });
export const unreadOne: string = en.t("unread", { count: 1 });
export const unreadMany: string = en.t("unread", { count: 1200 });
export const cartEmpty: string = en.t("cartEmpty", {});

// A key held in a variable still resolves to that key's arguments, which is
// what says the check survives the indirection an application actually writes.
const key = "greeting";
export const throughAVariable: string = en.t(key, { name: "Grace" });

// And the two other things a catalogue answers, so that a change narrowing `t`
// into uselessness cannot pass by leaving the rest of the type alone.
export const source: string = en.sourceOf("unread");
export const locale: string = en.locale;
export const untranslated: $ReadOnlyArray<string> = en.untranslated;
