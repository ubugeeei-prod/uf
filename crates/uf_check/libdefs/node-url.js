/**
 * @fileoverview Node's `url` module, with `pathToFileURL` answering the WHATWG
 * `URL` it returns.
 *
 * The vendored `node.js` types `pathToFileURL()` as the legacy
 * `url$urlObject`, whose `href` is optional, and `fileURLToPath()` as taking
 * that object or a string but not a `URL`. Node has returned a WHATWG `URL`
 * from `pathToFileURL` since it was added, and `fileURLToPath` takes one. So
 * `pathToFileURL(path).href` was "void is incompatible with string", and
 * `fileURLToPath(new URL(...))` was refused (about 12 errors in this
 * repository, ubugeeei-prod/uf#1451).
 *
 * The module is repeated **whole**, since a later `declare module` replaces an
 * earlier one. Only the two functions change. `node:url` is declared as
 * `$Exports<'url'>`, so it follows.
 */

// The global WHATWG `URL`, named here because inside the module `URL` means
// the module's own class.
type url$WhatwgURL = URL;

declare module "url" {
  declare type Url = {|
    protocol: string | null,
    slashes: boolean | null,
    auth: string | null,
    host: string | null,
    port: string | null,
    hostname: string | null,
    hash: string | null,
    search: string | null,
    query: string | null | { [string]: string, ... },
    pathname: string | null,
    path: string | null,
    href: string,
  |}

  declare type UrlWithStringQuery = {|
    ...Url,
    query: string | null
  |}

  declare type UrlWithParsedQuery = {|
    ...Url,
    query: { [string]: string, ... }
  |}

  declare function parse(urlStr: string, parseQueryString: true, slashesDenoteHost?: boolean): UrlWithParsedQuery;
  declare function parse(urlStr: string, parseQueryString?: false | void, slashesDenoteHost?: boolean): UrlWithStringQuery;
  declare function parse(urlStr: string, parseQueryString?: boolean, slashesDenoteHost?: boolean): Url;
  declare function format(urlObj: url$urlObject): string;
  declare function resolve(from: string, to: string): string;
  declare function domainToASCII(domain: string): string;
  declare function domainToUnicode(domain: string): string;
  /** A `file:` URL for `path`: a WHATWG `URL`, as Node returns. */
  declare function pathToFileURL(path: string, options?: { windows?: boolean, ... }): url$WhatwgURL;
  /** The path a `file:` URL names, from a string, a WHATWG `URL` or a legacy URL object. */
  declare function fileURLToPath(path: url$WhatwgURL | url$urlObject | string, options?: { windows?: boolean, ... }): string;
  declare class URLSearchParams {
    @@iterator(): Iterator<[string, string]>;

    size: number;

    constructor(init?: string | URLSearchParams | Array<[string, string]> | { [string]: string, ... } ): void;
    append(name: string, value: string): void;
    delete(name: string, value?: void): void;
    entries(): Iterator<[string, string]>;
    forEach<This>(callback: (this : This, value: string, name: string, searchParams: URLSearchParams) => mixed, thisArg?: This): void;
    get(name: string): string | null;
    getAll(name: string): string[];
    has(name: string, value?: string): boolean;
    keys(): Iterator<string>;
    set(name: string, value: string): void;
    sort(): void;
    values(): Iterator<string>;
    toString(): string;
  }
  declare class URL {
    static canParse(url: string, base?: string): boolean;
    static createObjectURL(blob: Blob): string;
    static createObjectURL(mediaSource: MediaSource): string;
    static revokeObjectURL(url: string): void;
    constructor(input: string, base?: string | URL): void;
    hash: string;
    host: string;
    hostname: string;
    href: string;
    readonly origin: string;
    password: string;
    pathname: string;
    port: string;
    protocol: string;
    search: string;
    readonly searchParams: URLSearchParams;
    username: string;
    toString(): string;
    toJSON(): string;
  }
}
