// @flow
//
// What `@uniflowed/std` infers, and the test that says so.
//
// Every refusal in this file is a type error `uf check` must raise, suppressed
// where it stands. The claim these modules make is that a generic container, a
// typed error walk and a typed context key all come back at the type the *call
// site* implies, with nothing annotated and no `any` underneath — and a claim
// about inference cannot be proved by running anything. It is proved the only
// way it can be: by running the checker and reading what it said.
//
// # How it is read
//
// A `// $FlowExpectedError[code]` comment says that the line after it must be
// reported with that code, and the words after the code say what the report is
// about. Flow suppresses the error, so `uf check` at the repository root stays
// clean, and a suppression that stops matching an error is reported as unused,
// which fails the test. A line without one must not be reported at all — so a
// change that makes any of these *stop* being an error fails the test, and so
// does one that makes something else here start being one. The tail of the file
// is the other half of the claim: every correct use is silent, without which a
// package whose every export was `any` would pass this too.
//
// # Why it is checked with the package rather than on its own
//
// `uf check` builds its module map out of the files it is asked to check, and a
// relative import that leaves that set resolves to an any-typed value — after
// which every type below is `any`, every line passes, and the test would prove
// the opposite of what it claims. So the test runs
// `uf check tests/type-tests npm/std`, with both in one set.
// `tests/type-tests/anchoring.js` says the rest of why these fixtures live here
// rather than inside the packages they are about.

import { decode as decodeBase32, encode as encodeBase32 } from "../../npm/std/base32.js";
import {
  BufferedReader,
  Scanner,
  TokenTooLongError,
  newReader as newBufferedReader,
  scanWords,
} from "../../npm/std/bufio.js";
import { Cursor, putVarint, uvarint } from "../../npm/std/binary.js";
import { Builder, compare, equal, split } from "../../npm/std/bytes.js";
import { background, key, withCancel, withTimeout, withValue } from "../../npm/std/context.js";
import { parse as parseCsv, stringify as stringifyCsv } from "../../npm/std/csv.js";
import { as, is, join, wrap } from "../../npm/std/errors.js";
import { GlobPattern, glob, matchGlob } from "../../npm/std/glob.js";
import { crc32, fnv1a64 } from "../../npm/std/hash.js";
import { Heap, heapify } from "../../npm/std/heap.js";
import { decode, encode } from "../../npm/std/hex.js";
import {
  BufferWriter,
  ShortWriteError,
  copy as copyBytes,
  limitReader,
  readAll as readAllBytes,
  readerFromBytes,
} from "../../npm/std/io.js";
import { List } from "../../npm/std/list.js";
import {
  isAbsolute as pathIsAbsolute,
  join as joinPath,
  relative as relativePath,
} from "../../npm/std/path.js";
import { binarySearch, binarySearchBy, search } from "../../npm/std/slices.js";
import { Group, Mutex, Semaphore, once } from "../../npm/std/sync.js";
import {
  InvalidHeaderError,
  append as appendHeader,
  canonicalHeaderKey,
  get as getHeader,
  parseHeaderBlock,
  parseHeaders,
  set as setHeader,
  stringifyHeaderBlock,
  values as headerValues,
} from "../../npm/std/textproto.js";
import type { HeaderMap } from "../../npm/std/textproto.js";
import { Duration, Ticker, Timer, after, milliseconds, seconds } from "../../npm/std/time.js";
import { TarReader, TarWriter } from "../../npm/std/tar.js";
import type { TarEntry } from "../../npm/std/tar.js";
import {
  ZipReader,
  ZipWriter,
  deflate as zipDeflate,
  inflate as zipInflate,
} from "../../npm/std/zip.js";
import type { ZipEntry } from "../../npm/std/zip.js";

class HttpError extends Error {
  status: number;
  constructor(status: number) {
    super("http");
    this.status = status;
  }
}

const failure: mixed = wrap("loading", new HttpError(429));

// --- errors -----------------------------------------------------------------
//
// `as` is the export whose whole value is the type it hands back. If it came
// back as `mixed` a caller would cast, and a cast is what this module exists to
// remove.

// $FlowExpectedError[incompatible-type] HttpError
export const asIsNotANumber: number | null = as(failure, HttpError);

// The field is reachable at the type the class declared it, without a cast.
const found = as(failure, HttpError);
// $FlowExpectedError[incompatible-type] number is incompatible with string
export const statusIsNotAString: string = found == null ? "" : found.status;

// A question, not a value: `is` answers a boolean and nothing wider.
// $FlowExpectedError[incompatible-type] boolean is incompatible with
export const isAnswersABoolean: string = is(failure, failure);

// `join` answers an error or nothing, so a caller has to look before throwing.
// $FlowExpectedError[incompatible-type] null
export const joinCanBeNothing: Error = join(new Error("a"));

// --- heap -------------------------------------------------------------------
//
// The comparator is the only place `T` appears in the constructor, so this is
// the load-bearing inference: get it wrong and every heap in a program is a
// heap of `any`.

const numbers = new Heap((a: number, b: number) => a - b);

// $FlowExpectedError[incompatible-type] number is incompatible with string
export const heapPopsItsElement: string | void = numbers.pop();

// The comparator's parameters are checked against what the heap holds, so a
// method the element does not have is refused where it is written.
// $FlowExpectedError[prop-missing] toUpperCase is missing in Number
export const comparatorSeesTheElement: mixed = new Heap<number>((a, b) => a.toUpperCase() - b);

// `heapify` infers from the array as well as from the comparator.
const words = heapify(["b", "a"], (a, b) => (a < b ? -1 : 1));
// $FlowExpectedError[incompatible-type] string is incompatible with number
export const heapifyKeepsTheElement: number | void = words.peek();

// --- list -------------------------------------------------------------------

const pages = new List<string>(["home"]);
const page = pages.pushFront("inbox");

// $FlowExpectedError[incompatible-type] string is incompatible with number
export const listElementHasItsValueType: number = page.value();

// $FlowExpectedError[incompatible-type] 1 is incompatible with string
export const listTakesItsElementType: mixed = pages.pushBack(1);

// --- context ----------------------------------------------------------------
//
// A key carries the type of the value stored under it. If it did not, a context
// would be a `Map<string, mixed>` with extra steps.

const TRACE = key<string>("trace");
const ATTEMPT = key<number>("attempt");
const root = background();

// $FlowExpectedError[incompatible-type] string is incompatible with number
export const keyReadsAtItsType: number | void = withValue(root, TRACE, "abc").value(TRACE);

// And the write is checked against the same type.
// $FlowExpectedError[incompatible-type] 7 is incompatible with string
export const keyWritesAtItsType: mixed = withValue(root, TRACE, 7);

// Two keys of different types are not interchangeable at a read.
// $FlowExpectedError[incompatible-type] number is incompatible with string
export const keysDoNotUnify: string | void = withValue(root, ATTEMPT, 1).value(ATTEMPT);

// The pair is a context and a cancel, in that order.
const [scope, cancel] = withCancel(root);
// $FlowExpectedError[incompatible-type] void is incompatible with number
export const cancelIsNotTheContext: number = cancel(scope);

// A deadline is milliseconds, not a Date.
// $FlowExpectedError[incompatible-type] Date
export const deadlineTakesMilliseconds: mixed = withTimeout(root, new Date());

// --- sync -------------------------------------------------------------------
//
// Every one of these passes a value through a lock or a limit, and the type has
// to survive the round trip.

const limit = new Semaphore(2);
// $FlowExpectedError[incompatible-type] 1 is incompatible with string
export const permitReturnsTheTask: Promise<string> = limit.withPermit(() => 1);

const mutex = new Mutex();
// $FlowExpectedError[incompatible-type] 1 is incompatible with string
export const lockReturnsTheBody: Promise<string> = mutex.withLock(async () => 1);

const connect = once(() => ({ port: 5432 }));
// $FlowExpectedError[incompatible-type] number is incompatible with string
export const onceKeepsItsValue: string = connect().port;

const rows = new Group<number>();
// $FlowExpectedError[incompatible-type] "not a number" is incompatible with number
export const groupTakesItsElement: mixed = rows.go(() => "not a number");

// --- bytes and encodings ----------------------------------------------------

const left = new Uint8Array(2);

// $FlowExpectedError[incompatible-type] string
export const equalTakesBytes: boolean = equal(left, "not bytes");

// $FlowExpectedError[incompatible-type] is incompatible with number literal 2
export const compareIsAnOrdering: 2 = compare(left, left);

// A split gives views the caller may read and may not grow.
// $FlowExpectedError[prop-missing] push is missing in $ReadOnlyArray
export const splitIsReadOnly: mixed = split(left, left).push(left);

// $FlowExpectedError[incompatible-type] Uint8Array
export const decodeGivesBytes: string = decode("dead");

// $FlowExpectedError[incompatible-type] string, a primitive, cannot be used as a subtype of Uint8Array
export const encodeGivesText: Uint8Array = encode(left);

// $FlowExpectedError[incompatible-type] Uint8Array
export const base32DecodeGivesBytes: string = decodeBase32("MY======");

// $FlowExpectedError[incompatible-type] string, a primitive, cannot be used as a subtype of Uint8Array
export const base32EncodeGivesText: Uint8Array = encodeBase32(left);

// A builder writes bytes; a string is the other method.
// $FlowExpectedError[incompatible-type] string
export const builderWritesBytes: mixed = new Builder().write("text");

// $FlowExpectedError[incompatible-type] string is incompatible with number
export const csvCellsAreText: number = parseCsv("a")[0][0];

// $FlowExpectedError[incompatible-type] 1 is incompatible with string
export const csvStringifyTakesText: mixed = stringifyCsv([[1]]);

const binaryCursor = new Cursor(left);

// $FlowExpectedError[incompatible-type] number is incompatible with string
export const binaryCursorReadsNumbers: string = binaryCursor.getUint16();

// $FlowExpectedError[incompatible-type] not a number" is incompatible with number
export const binaryCursorWritesNumbers: mixed = binaryCursor.putUint32("not a number");

// $FlowExpectedError[incompatible-type] bigint is incompatible with number
export const uvarintReadsBigInts: number = uvarint(left).value;

// $FlowExpectedError[incompatible-type] 1 is incompatible with bigint
export const putVarintTakesBigInts: mixed = putVarint(1);

// --- io ---------------------------------------------------------------------

const ioReader = readerFromBytes(left);
const ioWriter = new BufferWriter();

// $FlowExpectedError[incompatible-type] string
export const ioReaderNeedsBytes: mixed = readerFromBytes("not bytes");

// $FlowExpectedError[incompatible-type] Promise
export const ioReadAllAnswersBytes: Uint8Array = readAllBytes(ioReader);

// $FlowExpectedError[incompatible-type] "ten" is incompatible with number
export const ioLimitTakesNumber: mixed = limitReader(ioReader, "ten");

// $FlowExpectedError[incompatible-type] Promise
export const ioCopyAnswersCount: string = copyBytes(ioWriter, readerFromBytes(left));

const shortWrite = new ShortWriteError(2, 1);
// $FlowExpectedError[incompatible-type] number is incompatible with string
export const ioShortWriteCountsAreNumbers: string = shortWrite.written;

// --- bufio ------------------------------------------------------------------

const bufferedReader = new BufferedReader(readerFromBytes(left));
const scanner = new Scanner(readerFromBytes(left), { split: scanWords });

// $FlowExpectedError[incompatible-type] Promise
export const bufioPeekIsAsync: Uint8Array = bufferedReader.peek(1);

export const bufioBufferSizeIsNumeric: mixed = new BufferedReader(readerFromBytes(left), {
  // $FlowExpectedError[incompatible-type] "large" is incompatible with number
  bufferSize: "large",
});

// $FlowExpectedError[incompatible-type] string is incompatible with number
export const bufioScannerTextIsString: number = scanner.text();

// $FlowExpectedError[incompatible-type] Promise
export const bufioScannerScanIsAsync: boolean = scanner.scan();

const tokenTooLong = new TokenTooLongError(1024);
// $FlowExpectedError[incompatible-type] number is incompatible with string
export const bufioTokenLimitIsNumeric: string = tokenTooLong.limit;

// --- textproto --------------------------------------------------------------

const headers = parseHeaders("x-trace: one\n\n");
const headerError = new InvalidHeaderError("bad", 2, 3);

// $FlowExpectedError[incompatible-type] 1 is incompatible with string
export const textprotoKeyNeedsText: mixed = canonicalHeaderKey(1);

// $FlowExpectedError[incompatible-type] "" is incompatible with number
export const textprotoGetAnswersText: number = getHeader(headers, "x-trace") ?? "";

// $FlowExpectedError[incompatible-type] 1 is incompatible with string
export const textprotoSetValueNeedsText: mixed = setHeader(headers, "x-trace", 1);

// $FlowExpectedError[incompatible-type] number is incompatible with string
export const textprotoErrorLineIsNumeric: string = headerError.line;

// --- zip --------------------------------------------------------------------

const zipReader = new ZipReader(left);
const zipWriter = new ZipWriter();

// $FlowExpectedError[incompatible-type] Promise
export const zipReadIsAsync: Uint8Array = zipReader.read("file.txt");

// $FlowExpectedError[incompatible-type] 1 is incompatible with string
export const zipAddPathNeedsText: mixed = zipWriter.add(1, left);

export const zipAddCompressionIsChecked: mixed = zipWriter.add("file.txt", left, {
  // $FlowExpectedError[incompatible-type] "brotli" is incompatible with ZipCompression
  compression: "brotli",
});

// $FlowExpectedError[incompatible-type] Promise
export const zipDeflateIsAsync: Uint8Array = zipDeflate(left);

// --- tar --------------------------------------------------------------------

const tarReader = new TarReader(left);
const tarWriter = new TarWriter();

// $FlowExpectedError[incompatible-type] Uint8Array<ArrayBufferLike> is incompatible with string
export const tarReadAnswersBytes: string = tarReader.read("file.txt");

// $FlowExpectedError[incompatible-type] 1 is incompatible with string
export const tarAddPathNeedsText: mixed = tarWriter.add(1, left);

export const tarAddKindIsChecked: mixed = tarWriter.add("file.txt", left, {
  // $FlowExpectedError[incompatible-type] "symlink" is incompatible with TarEntryKind
  kind: "symlink",
});

// --- slices -----------------------------------------------------------------

// $FlowExpectedError[incompatible-type] number is incompatible with string
export const searchAnswersAnIndex: string = search(4, (index) => index >= 2);

// The sorted array decides the element type, not `any`.
const sortedNumbers: $ReadOnlyArray<number> = [1, 3, 5];
const compareNumbers = (left: number, right: number): number => left - right;
const searchSortedNumbers = (target: number) => binarySearch(sortedNumbers, target, compareNumbers);
const searchNumbers = binarySearch(sortedNumbers, 3, (left, right) => left - right);
// $FlowExpectedError[incompatible-type] boolean is incompatible with number
export const binarySearchFoundIsBoolean: number = searchNumbers.found;

// $FlowExpectedError[incompatible-type] "3" is incompatible with number
export const binarySearchTargetHasElementType: mixed = searchSortedNumbers("3");

const searchRows = binarySearchBy([{ id: "a" }, { id: "b" }], (row) => row.id.localeCompare("b"));
// $FlowExpectedError[incompatible-type] number is incompatible with string
export const binarySearchByIndexIsNumber: string = searchRows.index;

// --- path -------------------------------------------------------------------

// $FlowExpectedError[incompatible-type] string is incompatible with number
export const pathJoinAnswersText: number = joinPath("app", "routes");

// $FlowExpectedError[incompatible-type] boolean is incompatible with string
export const pathAbsoluteAnswersBoolean: string = pathIsAbsolute("/app");

// $FlowExpectedError[incompatible-type] 1 is incompatible with string
export const pathJoinTakesText: mixed = joinPath("app", 1);

// --- glob -------------------------------------------------------------------

const javascriptFiles = glob("src/**/*.js");
const compiledGlob: GlobPattern = javascriptFiles;

// $FlowExpectedError[incompatible-type] boolean is incompatible with string
export const globAnswersBoolean: string = javascriptFiles.match("src/index.js");

// $FlowExpectedError[incompatible-type] 1 is incompatible with string
export const globPatternNeedsText: mixed = glob(1);

// --- hash -------------------------------------------------------------------

// $FlowExpectedError[incompatible-type] number is incompatible with string
export const crc32AnswersANumber: string = crc32(left);

// $FlowExpectedError[incompatible-type] bigint is incompatible with number
export const fnv1a64AnswersABigInt: number = fnv1a64("hello");

// $FlowExpectedError[incompatible-type] 1 is incompatible with
export const hashInputIsBytesOrText: mixed = crc32(1);

// --- time -------------------------------------------------------------------

const oneSecond = seconds(1);
const timer = new Timer(milliseconds(10));
const ticker = new Ticker(milliseconds(10));

// $FlowExpectedError[incompatible-type] number is incompatible with string
export const durationMillisecondsAreNumbers: string = oneSecond.milliseconds();

// $FlowExpectedError[incompatible-type] number
export const timerDelayIsDurationOrMillis: mixed = new Timer("soon");

// $FlowExpectedError[incompatible-type] Promise
export const timerDoneIsAPromise: boolean = timer.done();

// $FlowExpectedError[incompatible-type] number
export const tickerTicksCanEnd: Promise<number> = ticker.tick();

// $FlowExpectedError[incompatible-type] number
export const afterDelayIsDurationOrMillis: mixed = after("soon");

// --- what is *not* an error -------------------------------------------------
//
// Without these the fixture would pass just as happily on a package whose every
// export was `any`.

export const asNarrows: HttpError | null = as(failure, HttpError);
export const isAsks: boolean = is(failure, failure);
export const joinIsNullable: Error | null = join(new Error("a"));
export const heapPops: number | void = numbers.pop();
export const heapifyInfers: string | void = words.peek();
export const drainYieldsElements: Array<number> = [...numbers.drain()];
export const listFront: string | void = pages.front()?.value();
export const listValues: Array<string> = [...pages.values()];
export const keyReads: string | void = withValue(root, TRACE, "abc").value(TRACE);
export const attemptReads: number | void = withValue(root, ATTEMPT, 1).value(ATTEMPT);
export const cancelTakesNothing: void = cancel();
export const cancelTakesAReason: void = cancel(new Error("gone"));
export const scopeHasASignal: AbortSignal = scope.signal();
export const permitPasses: Promise<number> = limit.withPermit(() => 1);
export const lockPasses: Promise<number> = mutex.withLock(async () => 1);
export const onceKeeps: number = connect().port;
export const groupWaits: Promise<$ReadOnlyArray<number>> = rows.wait();
export const bytesCompare: -1 | 0 | 1 = compare(left, left);
export const bytesSplit: $ReadOnlyArray<Uint8Array> = split(left, left);
export const hexEncodes: string = encode(left);
export const hexDecodes: Uint8Array = decode("dead");
export const base32Encodes: string = encodeBase32(left);
export const base32Decodes: Uint8Array = decodeBase32("MY======");
export const csvRows: $ReadOnlyArray<$ReadOnlyArray<string>> = parseCsv("a,b");
export const csvText: string = stringifyCsv([["a", "b"]]);
export const binaryCursorOffset: number = binaryCursor.offset();
export const binaryCursorRead: number = binaryCursor.getUint16();
export const binaryUvarint: bigint = uvarint(left).value;
export const binaryPutVarint: Uint8Array = putVarint(-1n);
export const ioReadAllBytes: Promise<Uint8Array> = readAllBytes(readerFromBytes(left));
export const ioCopyCount: Promise<number> = copyBytes(ioWriter, readerFromBytes(left));
export const ioBufferBytes: Uint8Array = new BufferWriter().bytes();
export const bufioReader: BufferedReader = newBufferedReader(readerFromBytes(left));
export const bufioPeek: Promise<Uint8Array> = bufioReader.peek(1);
export const bufioScannerBytes: Uint8Array = scanner.bytes();
export const bufioScannerText: string = scanner.text();
export const bufioScannerScan: Promise<boolean> = scanner.scan();
export const textprotoHeaders: HeaderMap = headers;
export const textprotoRest: string = parseHeaderBlock("x: y\n\nbody").rest;
export const textprotoKey: string = canonicalHeaderKey("content-type");
export const textprotoFirst: string | null = getHeader(headers, "x-trace");
export const textprotoValues: $ReadOnlyArray<string> = headerValues(headers, "x-trace");
export const textprotoChanged: HeaderMap = appendHeader(
  setHeader(headers, "x-trace", "two"),
  "x-trace",
  "three",
);
export const textprotoBlock: string = stringifyHeaderBlock(headers);
export const zipEntries: $ReadOnlyArray<ZipEntry> = zipReader.entries();
export const zipInflatedBytes: Promise<Uint8Array> = zipInflate(left);
export const zipAddedEntry: Promise<ZipEntry> = zipWriter.add("file.txt", left, {
  compression: "store",
});
export const tarEntries: $ReadOnlyArray<TarEntry> = tarReader.entries();
export const tarBytes: Uint8Array = tarReader.read("file.txt");
export const tarAddedEntry: TarEntry = tarWriter.add("file.txt", left);
export const searchIndex: number = search(4, (index) => index >= 2);
export const binarySearchResult: { readonly index: number, readonly found: boolean } = binarySearch(
  sortedNumbers,
  3,
  (left, right) => left - right,
);
export const binarySearchByResult: { readonly index: number, readonly found: boolean } =
  binarySearchBy([{ id: "a" }, { id: "b" }], (row) => row.id.localeCompare("b"));
export const pathJoinText: string = joinPath("app", "routes", "..", "assets");
export const pathRelativeText: string = relativePath("app/routes", "app/assets");
export const pathAbsoluteBoolean: boolean = pathIsAbsolute("/app");
export const globPattern: GlobPattern = compiledGlob;
export const globMatchBoolean: boolean = matchGlob(compiledGlob, "src/index.js");
export const crc32Number: number = crc32(left);
export const fnv64BigInt: bigint = fnv1a64("hello");
export const durationValue: Duration = oneSecond;
export const durationCompare: -1 | 0 | 1 = oneSecond.compare(500);
export const timerDone: Promise<boolean> = timer.done();
export const tickerTick: Promise<number | null> = ticker.tick();
export const afterPromise: Promise<void> = after(milliseconds(1));
