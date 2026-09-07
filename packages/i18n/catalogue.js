// @flow
//
// `@uniflowed/i18n/catalogue`: the messages an application has, and the types
// that make calling one checkable.
//
// # The problem this module exists to solve
//
// Every i18n library types the key and gives up on the arguments. `t("unread",
// { count })` and `t("unread", {})` are the same call to the checker, so a
// message that gained a placeholder in the source locale keeps compiling
// everywhere it is used and renders `{$count}` to a user. That is the bug this
// module is about, and it is not a small one: the placeholder was added by
// whoever wrote the English, and the call sites are wherever the message is
// shown.
//
// The reason libraries stop there is real. Reading `{name: string, count:
// number}` off the *string* `"Hello {$name}, you have {$count}"` needs the
// message parsed at the type level, which in TypeScript means template-literal
// types and in Flow is not possible at all: Flow has no template-literal
// types, so the placeholders inside a string literal are not part of its type
// in any form a conditional type can reach.
//
// # So the parameters are values
//
// A message declares its parameters beside its source, as values:
//
//     const messages = {
//       greeting: message("Hello, {$name}!", { name: string }),
//       unread: message(UNREAD_SOURCE, { count: number }),
//       cartEmpty: message("Your cart is empty.", {}),
//     };
//
// `ParamArgs` maps that object to `{ name: string }` and `{ count: number }`,
// `ArgsOf` reads it back out of the message, and `t` is generic in the key —
// so `t("greeting", { name })` checks, `t("greeting", {})` does not, and
// `t("unread", { count: "3" })` does not either. Nobody wrote a type down.
//
// Declaring the parameters as values rather than as a type argument is what
// closes the gap the type system leaves open, and this is the whole reason for
// the shape. `message` is handed both the source *and* the parameters at run
// time, so it can compare them: a message that reads `$nom` when the
// parameters say `name` throws where it is written, naming the key. A type
// argument — `message<{ name: string }>("Hello, {$nom}!")` — would have been
// less to write and would have checked nothing, because a type argument is
// erased before anything could look at it.
//
// The same comparison catches the annotation: `{ count: number }` against a
// message that says `{$count :date}` is refused. That is not a type error
// anywhere, in any checker; it is two facts about one name that only exist in
// the same place at this moment.
//
// # What is checked, and where
//
// | | |
// | --- | --- |
// | a key that does not exist | Flow, at the call |
// | a missing or misspelled argument | Flow, at the call |
// | an argument of the wrong type | Flow, at the call |
// | a message reading a parameter nobody declared | `message`, where it is written |
// | a parameter the message never reads | `message`, where it is written |
// | a parameter annotated as the wrong kind | `message`, where it is written |
// | a translation that lost or invented a placeholder | `translate`, at start-up |
// | a translation key the source locale does not have | `translate`, at start-up |
// | a message never translated | reported as `untranslated`, never a throw |
//
// The one thing nothing here checks is that a *translation* says the same
// thing as its source. That is what a translator is for.
//
// # Why `t("cartEmpty", {})` and not `t("cartEmpty")`
//
// The empty object is not an oversight. Flow cannot make one parameter
// optional as a function of another parameter's type, so the choice is between
// an argument that is always required and an argument that is always optional
// — and the second gives up the whole point of the module, because
// `t("greeting")` would then check too. Two extra characters on the messages
// that take nothing is the cheaper half of that trade.

import type { FormatContext } from "./format.js";
import { MessageFormatError, formatMessage, isKnownFunction } from "./format.js";
import type { MessageNode, MessageUsage } from "./syntax.js";
import { messageUsage, parseMessage } from "./syntax.js";

/** What a message says it needs. */
export type ParamKind = "string" | "number" | "boolean" | "date";

type ParamCarrier<out TValue> = {
  readonly kind: ParamKind,
  /**
   * A phantom.
   *
   * `TValue` is never produced or consumed at run time — a parameter
   * declaration is four bytes of metadata — and it has to appear somewhere in
   * the type to be a parameter at all. Return position keeps it covariant,
   * which is what lets `Param<string>` be used where `Param<mixed>` is wanted
   * and so lets `ParamMap` have an indexer.
   */
  readonly value: () => TValue,
};

/** A parameter's kind, carrying the type of the value it stands for. */
export opaque type Param<out TValue>: ParamCarrier<TValue> = ParamCarrier<TValue>;

/** The parameters of a message: what `message` takes beside its source. */
export type ParamMap = { readonly [string]: Param<mixed>, ... };

/** The value type behind one parameter. */
export type ParamValue<TParam> = TParam extends Param<infer TValue> ? TValue : empty;

/** The argument object a set of parameters describes. */
export type ParamArgs<TParams extends ParamMap> = {
  [Key in keyof TParams]: ParamValue<TParams[Key]>,
};

/**
 * Never called, and unreachable rather than absent.
 *
 * A phantom needs a value of a function type, and `undefined` cast into one
 * would be a lie the first time somebody called it by accident. This throws
 * something that says what happened.
 */
function phantom(): empty {
  throw new Error("@uniflowed/i18n: a parameter declaration is not a value and cannot be called");
}

/** A parameter formatted as text: `{$name}`, or `{$name :string}`. */
export const string: Param<string> = { kind: "string", value: phantom };

/** A parameter formatted as a number: `{$count :number}`, `{$n :integer}`. */
export const number: Param<number> = { kind: "number", value: phantom };

/** A parameter that selects but rarely prints: `.match $isAdmin`. */
export const boolean: Param<boolean> = { kind: "boolean", value: phantom };

/** A parameter formatted as an instant: `{$at :date}`, `:time`, `:datetime`. */
export const date: Param<Date> = { kind: "date", value: phantom };

/** Which MF2 functions may be applied to a parameter of each kind. */
const KINDS_TO_FUNCTIONS: { readonly [ParamKind]: $ReadOnlyArray<string> } = {
  string: ["string"],
  number: ["number", "integer"],
  boolean: ["string"],
  date: ["date", "time", "datetime"],
};

type MessageCarrier<out TArgs> = {
  readonly source: string,
  readonly node: MessageNode,
  readonly usage: MessageUsage,
  readonly params: ParamMap,
  /** A phantom, for the same reason `ParamCarrier`'s is. */
  readonly args: () => TArgs,
};

/**
 * One message: its MF2 source, parsed, and the arguments formatting it needs.
 *
 * Opaque so that the only way to get one is [`message`], which is the only
 * thing that checks the source against the parameters. A hand-written object
 * of the same shape would be a message whose type says one thing and whose
 * text says another, which is the single failure this module exists to
 * prevent.
 */
export opaque type Message<out TArgs>: MessageCarrier<TArgs> = MessageCarrier<TArgs>;

/** The arguments one message needs. */
export type ArgsOf<TMessage> = TMessage extends Message<infer TArgs> ? TArgs : empty;

/** A catalogue's messages: the object `defineCatalogue` is given. */
export type MessageMap = { readonly [string]: Message<mixed>, ... };

/** One locale's text for every key, as plain strings a translator can edit. */
export type Translations<TMessages extends MessageMap> = {
  [Key in keyof TMessages]: string,
};

/**
 * A message that does not agree with the parameters declared beside it.
 *
 * Separate from `MessageSyntaxError` because the message parses fine: this is
 * the check no type system performs, and an error that says "unexpected token"
 * would send the reader looking for a typo in the syntax.
 */
export class MessageContractError extends Error {
  key: string;

  constructor(key: string, detail: string) {
    super(`@uniflowed/i18n: the message ${JSON.stringify(key)} ${detail}`);
    this.name = "MessageContractError";
    this.key = key;
  }
}

function listed(names: $ReadOnlyArray<string>): string {
  return names.length === 0 ? "nothing" : names.map((name) => `$${name}`).join(", ");
}

/**
 * Hold a parsed message against the parameters declared beside it.
 *
 * `key` is only for the error text — a message is checked long before it is
 * put in a catalogue, and `message` passes the source itself so that the
 * failure reads sensibly for a message that has no key yet.
 */
function checkAgainstParams(key: string, usage: MessageUsage, params: ParamMap): void {
  for (const name of usage.functions) {
    if (!isKnownFunction(name)) {
      throw new MessageContractError(
        key,
        `names the function :${name}, which uf does not implement; the six it does are ` +
          ":string, :number, :integer, :date, :time and :datetime",
      );
    }
  }

  const declared = Object.keys(params).sort();
  const missing = usage.variables.filter((name) => !declared.includes(name));
  if (missing.length > 0) {
    throw new MessageContractError(
      key,
      `reads ${listed(missing)}, which ${missing.length === 1 ? "is" : "are"} not declared in ` +
        `its parameters (${listed(declared)})`,
    );
  }

  const unused = declared.filter((name) => !usage.variables.includes(name));
  if (unused.length > 0) {
    // Refused rather than tolerated. An unused parameter is nearly always the
    // other half of a rename that only landed in one of the two places, and a
    // caller is being made to pass a value that reaches nothing.
    throw new MessageContractError(
      key,
      `declares ${listed(unused)}, which the message never reads`,
    );
  }

  for (const [name, functionName] of usage.annotated) {
    const param = params[name];
    if (param == null) continue;
    const allowed = KINDS_TO_FUNCTIONS[param.kind];
    if (!allowed.includes(functionName)) {
      throw new MessageContractError(
        key,
        `applies :${functionName} to $${name}, which is declared ${param.kind}; ` +
          `${param.kind} takes ${allowed.map((each) => `:${each}`).join(" or ")}`,
      );
    }
  }
}

/**
 * Declare a message and the parameters formatting it needs.
 *
 * Throws where the message is written if the two disagree — see the module
 * header for why that is the point rather than a nicety.
 */
export function message<TParams extends ParamMap>(
  source: string,
  params: TParams,
): Message<ParamArgs<TParams>> {
  const node = parseMessage(source);
  const usage = messageUsage(node);
  checkAgainstParams(source, usage, params);
  return { source, node, usage, params, args: phantom };
}

/**
 * An application's messages in one locale, and the typed way to format one.
 *
 * A plain object rather than an opaque type: everything on it is worth
 * reading, `translate` builds a second one from the first, and nothing breaks
 * if an application builds one itself — the guarantee lives in [`Message`],
 * which a catalogue can only hold and never mint.
 */
export type Catalogue<TMessages extends MessageMap> = {
  readonly locale: string,
  /** Keys this locale did not translate, so they fall back to the source. */
  readonly untranslated: $ReadOnlyArray<string>,
  readonly t: <TKey extends $Keys<TMessages>>(key: TKey, args: ArgsOf<TMessages[TKey]>) => string,
  /** The MF2 source behind a key, for a dev overlay or a test. */
  readonly sourceOf: (key: string) => string,
  /** What `translate` needs and nothing else should read. */
  readonly messages: TMessages,
};

export type CatalogueOptions = {
  /**
   * What to do about a value that reached formatting and should not have.
   *
   * Throws by default. Every ordinary mistake is caught before this — by Flow
   * at the call, or by `message` where the message is written — so reaching
   * here means a value came from outside the type system and is not what the
   * message needs. That is worth finding rather than papering over, and an
   * application that would rather show the MF2 fallback than fail a render
   * passes something that records instead.
   */
  readonly onError?: (error: MessageFormatError) => void,
  /** See `FormatContext` in `format.js`; off by default, and why. */
  readonly bidiIsolation?: boolean,
};

function raise(error: MessageFormatError): void {
  throw error;
}

function contextFor(locale: string, options: CatalogueOptions): FormatContext {
  return {
    locale,
    bidiIsolation: options.bidiIsolation === true,
    onError: options.onError ?? raise,
    numberFormats: new Map(),
    dateFormats: new Map(),
    pluralRules: new Map(),
  };
}

/**
 * Build a catalogue over a set of messages.
 *
 * The messages are already parsed — [`message`] did that — so this allocates
 * one lookup table and three empty caches. Defining a catalogue at module
 * scope is cheap on purpose: the parse happened when the message was
 * declared, and a page that never formats anything pays for nothing beyond it.
 */
export function defineCatalogue<TMessages extends MessageMap>(
  locale: string,
  messages: TMessages,
  options?: CatalogueOptions,
): Catalogue<TMessages> {
  return build(locale, messages, messages, [], options ?? {});
}

/**
 * The one place a catalogue is actually constructed.
 *
 * `messages` is what the types describe and `formatting` is what is formatted;
 * they are the same object for a source catalogue and differ for a translated
 * one. Keeping the two apart is what lets `translate` return
 * `Catalogue<TMessages>` — the same keys and the same argument types — over
 * different text, with no cast anywhere.
 */
function build<TMessages extends MessageMap>(
  locale: string,
  messages: TMessages,
  formatting: { readonly [string]: Message<mixed> },
  untranslated: $ReadOnlyArray<string>,
  options: CatalogueOptions,
): Catalogue<TMessages> {
  const context = contextFor(locale, options);
  const compiled: Map<string, Message<mixed>> = new Map();
  for (const key of Object.keys(formatting)) {
    compiled.set(key, formatting[key]);
  }

  const find = (key: string): Message<mixed> => {
    const found = compiled.get(key);
    if (found == null) {
      // Unreachable through the types, and worth a real error anyway: a key
      // read out of a database or a URL arrives as a string and Flow never saw
      // it.
      throw new MessageContractError(key, "is not in this catalogue");
    }
    return found;
  };

  // `mixed` rather than a dictionary, and not because the type is unknown to
  // the caller — it is `ArgsOf<TMessages[TKey]>`, and Flow checks the call
  // against it. It is unknown *here*: inside a generic body there is nothing to
  // resolve that conditional against, so a dictionary parameter would make the
  // one function this package needs to be polymorphic the one it cannot write.
  // `format.js` reads the arguments through a `Map` for the same reason and one
  // better one.
  const format = (key: string, args: mixed): string => formatMessage(find(key).node, args, context);

  return {
    locale,
    untranslated,
    t: format,
    sourceOf: (key: string) => find(key).source,
    messages,
  };
}

/**
 * A second locale over the same keys and the same argument types.
 *
 * A translation is plain strings, which is what a translator can be handed and
 * what a `.json` export can hold. It does not redeclare the parameters,
 * because they are a property of the message rather than of the language, and
 * a locale file that redeclared them would be a second place for them to
 * disagree.
 *
 * Partial on purpose: a locale that has translated nine keys of twenty is the
 * ordinary state of a growing application, and refusing to build a catalogue
 * for it would mean an untranslated string could never ship. The keys that
 * fell back are on the catalogue as `untranslated`, so a test can require the
 * list to be empty for the locales an application claims to support — which is
 * the same fact, asserted by whoever wants to assert it rather than by this
 * package.
 */
export function translate<TMessages extends MessageMap>(
  base: Catalogue<TMessages>,
  locale: string,
  translations: Partial<Translations<TMessages>>,
  options?: CatalogueOptions,
): Catalogue<TMessages> {
  const sources: { readonly [string]: mixed } = translations;
  const formatting: { [string]: Message<mixed> } = {};
  const untranslated: Array<string> = [];

  for (const key of Object.keys(sources)) {
    if (base.messages[key] == null) {
      throw new MessageContractError(
        key,
        `is translated into ${locale} and is not in the source catalogue`,
      );
    }
  }

  for (const key of Object.keys(base.messages)) {
    const original = base.messages[key];
    const replacement = sources[key];
    if (typeof replacement !== "string") {
      untranslated.push(key);
      formatting[key] = original;
      continue;
    }

    const node = parseMessage(replacement);
    const usage = messageUsage(node);
    // The parameters are the source message's, not a new set: a translation
    // may reorder placeholders, drop one that a language does not need, and
    // add none. Checking it against the source's parameters is what turns "the
    // German broke on a page nobody opened" into a start-up error naming the
    // key.
    checkAgainstParams(key, usage, original.params);
    formatting[key] = { source: replacement, node, usage, params: original.params, args: phantom };
  }

  return build(locale, base.messages, formatting, untranslated, options ?? {});
}

/** How a locale's translations arrive, usually `() => import("./ja.js")`. */
export type LocaleLoader<TMessages extends MessageMap> = () => Promise<
  Partial<Translations<TMessages>>,
>;

/**
 * Every locale an application has, with only the source one loaded.
 *
 * `load` is asynchronous and `available` is not, which is the split a server
 * needs: negotiation happens against the list, before anything is fetched, so
 * a request that resolves to a locale the page already has costs no round
 * trip.
 */
export type Locales<TMessages extends MessageMap> = {
  readonly source: Catalogue<TMessages>,
  readonly available: $ReadOnlyArray<string>,
  readonly load: (locale: string) => Promise<Catalogue<TMessages>>,
};

/**
 * Bind a set of lazily loaded translations to a source catalogue.
 *
 * The loaders are thunks rather than modules so that a bundler splits them:
 * `() => import("./ja.js")` is a chunk a page fetches only if negotiation
 * lands on Japanese, which is what "a page ships one locale" means in
 * practice. Passing the modules directly would put every locale in the entry
 * bundle and make this a table with extra steps.
 *
 * A locale is loaded at most once. The promise is cached rather than the
 * catalogue, so two concurrent requests for the same locale share one fetch
 * instead of racing to build two catalogues over one download.
 */
export function defineLocales<TMessages extends MessageMap>(
  source: Catalogue<TMessages>,
  loaders: { readonly [string]: LocaleLoader<TMessages> },
  options?: CatalogueOptions,
): Locales<TMessages> {
  const pending: Map<string, Promise<Catalogue<TMessages>>> = new Map();
  const available = [source.locale, ...Object.keys(loaders).filter((tag) => tag !== source.locale)];

  const load = (locale: string): Promise<Catalogue<TMessages>> => {
    if (locale === source.locale) return Promise.resolve(source);
    const started = pending.get(locale);
    if (started != null) return started;
    const loader = loaders[locale];
    if (loader == null) {
      return Promise.reject(
        new MessageContractError(
          locale,
          `is not a locale this application has; it has ${available.join(", ")}`,
        ),
      );
    }
    const promise = loader().then((strings) => translate(source, locale, strings, options));
    pending.set(locale, promise);
    return promise;
  };

  return { source, available, load };
}
