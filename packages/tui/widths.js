// @flow
//
// How many terminal columns a piece of text occupies.
//
// This is the first thing a terminal renderer has to get right and the easiest
// one to get wrong, because JavaScript offers a number that looks like the
// answer and is not: `"A界B".length` and `"ABC".length` are both `3`, and only
// one of those strings fits in three columns. Everything downstream — where a
// border's right edge lands, where a line wraps, which cell the cursor moves
// to — is computed from the answer here, so a width that is one column out
// does not produce a slightly wrong frame. It produces a frame whose every
// subsequent row is shifted.
//
// # Two units, not one
//
// A *grapheme cluster* is what a reader calls a character: a base scalar plus
// whatever combines onto it, up to and including a family emoji built from
// four people and three joiners. A *cell* is one column of one row. The
// mapping between them is many-to-many, and this module is the only place in
// the package that knows it. `Intl.Segmenter` does the clustering — it is in
// every runtime uf supports and it implements UAX #29, which is a standard
// nobody should be reimplementing.
//
// # Why the tables are copied rather than shared
//
// `crates/uf_term/src/text/tables.rs` holds exactly these two range lists, and
// `crates/uf_term/src/text.rs` implements exactly these rules, because the uf
// CLI has to answer the same question in Rust before any JavaScript is
// running. Two copies of a Unicode table is a thing to be uncomfortable about,
// so the discomfort is made mechanical instead of moral: `tui.test.js` parses
// the Rust file and asserts the ranges below are identical to it, and fails
// when either side is edited alone. The copy is therefore checked, and the
// alternative — shipping a native binding to a published Flow package so that
// `@uniflowed/tui` can ask Rust how wide `界` is — is a far larger price for
// the same answer.

const ZERO_WIDTH: $ReadOnlyArray<number> = [
  0x00ad, 0x00ad, 0x0300, 0x036f, 0x0483, 0x0489, 0x0591, 0x05bd, 0x05bf, 0x05bf, 0x05c1, 0x05c2,
  0x05c4, 0x05c5, 0x05c7, 0x05c7, 0x0610, 0x061a, 0x064b, 0x065f, 0x0670, 0x0670, 0x06d6, 0x06dc,
  0x06df, 0x06e4, 0x06e7, 0x06e8, 0x06ea, 0x06ed, 0x0711, 0x0711, 0x0730, 0x074a, 0x07a6, 0x07b0,
  0x07eb, 0x07f3, 0x0816, 0x0819, 0x081b, 0x0823, 0x0825, 0x0827, 0x0829, 0x082d, 0x0859, 0x085b,
  0x08e3, 0x0902, 0x093a, 0x093a, 0x093c, 0x093c, 0x0941, 0x0948, 0x094d, 0x094d, 0x0951, 0x0957,
  0x0962, 0x0963, 0x0981, 0x0981, 0x09bc, 0x09bc, 0x09c1, 0x09c4, 0x09cd, 0x09cd, 0x09e2, 0x09e3,
  0x0a01, 0x0a02, 0x0a3c, 0x0a3c, 0x0a41, 0x0a42, 0x0a47, 0x0a48, 0x0a4b, 0x0a4d, 0x0a70, 0x0a71,
  0x0abc, 0x0abc, 0x0ac1, 0x0ac5, 0x0ac7, 0x0ac8, 0x0acd, 0x0acd, 0x0b01, 0x0b01, 0x0b3c, 0x0b3c,
  0x0b3f, 0x0b3f, 0x0b41, 0x0b44, 0x0b4d, 0x0b4d, 0x0bc0, 0x0bc0, 0x0bcd, 0x0bcd, 0x0c00, 0x0c00,
  0x0c3e, 0x0c40, 0x0c46, 0x0c48, 0x0c4a, 0x0c4d, 0x0cbc, 0x0cbc, 0x0ccc, 0x0ccd, 0x0d41, 0x0d44,
  0x0d4d, 0x0d4d, 0x0dca, 0x0dca, 0x0e31, 0x0e31, 0x0e34, 0x0e3a, 0x0e47, 0x0e4e, 0x0eb1, 0x0eb1,
  0x0eb4, 0x0ebc, 0x0ec8, 0x0ecd, 0x0f35, 0x0f35, 0x0f37, 0x0f37, 0x0f39, 0x0f39, 0x0f71, 0x0f7e,
  0x0f80, 0x0f84, 0x0f86, 0x0f87, 0x102d, 0x1030, 0x1032, 0x1037, 0x1039, 0x103a, 0x1058, 0x1059,
  0x135d, 0x135f, 0x1712, 0x1714, 0x17b4, 0x17b5, 0x17b7, 0x17bd, 0x17c6, 0x17c6, 0x17c9, 0x17d3,
  0x180b, 0x180e, 0x18a9, 0x18a9, 0x1a17, 0x1a18, 0x1ab0, 0x1aff, 0x1b00, 0x1b03, 0x1b34, 0x1b34,
  0x1b6b, 0x1b73, 0x1dc0, 0x1dff, 0x200b, 0x200f, 0x202a, 0x202e, 0x2060, 0x2064, 0x206a, 0x206f,
  0x20d0, 0x20f0, 0x2cef, 0x2cf1, 0x302a, 0x302d, 0x3099, 0x309a, 0xa66f, 0xa672, 0xa674, 0xa67d,
  0xa69e, 0xa69f, 0xa806, 0xa806, 0xa8c4, 0xa8c5, 0xa8e0, 0xa8f1, 0xfb1e, 0xfb1e, 0xfe00, 0xfe0f,
  0xfe20, 0xfe2f, 0xfeff, 0xfeff, 0xfff9, 0xfffb, 0x101fd, 0x101fd, 0x1d167, 0x1d169, 0x1d17b,
  0x1d182, 0x1d185, 0x1d18b, 0x1d1aa, 0x1d1ad, 0x1d242, 0x1d244, 0xe0100, 0xe01ef,
];

const WIDE: $ReadOnlyArray<number> = [
  0x1100, 0x115f, 0x231a, 0x231b, 0x2329, 0x232a, 0x23e9, 0x23ec, 0x23f0, 0x23f0, 0x23f3, 0x23f3,
  0x25fd, 0x25fe, 0x2614, 0x2615, 0x2648, 0x2653, 0x267f, 0x267f, 0x2693, 0x2693, 0x26a1, 0x26a1,
  0x26aa, 0x26ab, 0x26bd, 0x26be, 0x26c4, 0x26c5, 0x26ce, 0x26ce, 0x26d4, 0x26d4, 0x26ea, 0x26ea,
  0x26f2, 0x26f3, 0x26f5, 0x26f5, 0x26fa, 0x26fa, 0x26fd, 0x26fd, 0x2705, 0x2705, 0x270a, 0x270b,
  0x2728, 0x2728, 0x274c, 0x274c, 0x274e, 0x274e, 0x2753, 0x2755, 0x2757, 0x2757, 0x2795, 0x2797,
  0x27b0, 0x27b0, 0x27bf, 0x27bf, 0x2b1b, 0x2b1c, 0x2b50, 0x2b50, 0x2b55, 0x2b55, 0x2e80, 0x2e99,
  0x2e9b, 0x2ef3, 0x2f00, 0x2fd5, 0x2ff0, 0x2ffb, 0x3000, 0x303e, 0x3041, 0x3096, 0x309b, 0x30ff,
  0x3105, 0x312f, 0x3131, 0x318e, 0x3190, 0x31e3, 0x31f0, 0x321e, 0x3220, 0x3247, 0x3250, 0x4dbf,
  0x4e00, 0xa48c, 0xa490, 0xa4c6, 0xa960, 0xa97c, 0xac00, 0xd7a3, 0xf900, 0xfaff, 0xfe10, 0xfe19,
  0xfe30, 0xfe52, 0xfe54, 0xfe66, 0xfe68, 0xfe6b, 0xff01, 0xff60, 0xffe0, 0xffe6, 0x16fe0, 0x16fe4,
  0x16ff0, 0x16ff1, 0x17000, 0x187f7, 0x18800, 0x18cd5, 0x1b000, 0x1b152, 0x1b164, 0x1b167, 0x1b170,
  0x1b2fb, 0x1f004, 0x1f004, 0x1f0cf, 0x1f0cf, 0x1f18e, 0x1f18e, 0x1f191, 0x1f19a, 0x1f200, 0x1f320,
  0x1f32d, 0x1f335, 0x1f337, 0x1f37c, 0x1f37e, 0x1f393, 0x1f3a0, 0x1f3ca, 0x1f3cf, 0x1f3d3, 0x1f3e0,
  0x1f3f0, 0x1f3f4, 0x1f3f4, 0x1f3f8, 0x1f43e, 0x1f440, 0x1f440, 0x1f442, 0x1f4fc, 0x1f4ff, 0x1f53d,
  0x1f54b, 0x1f54e, 0x1f550, 0x1f567, 0x1f57a, 0x1f57a, 0x1f595, 0x1f596, 0x1f5a4, 0x1f5a4, 0x1f5fb,
  0x1f64f, 0x1f680, 0x1f6c5, 0x1f6cc, 0x1f6cc, 0x1f6d0, 0x1f6d2, 0x1f6d5, 0x1f6d7, 0x1f6eb, 0x1f6ec,
  0x1f6f4, 0x1f6fc, 0x1f7e0, 0x1f7eb, 0x1f90c, 0x1f93a, 0x1f93c, 0x1f945, 0x1f947, 0x1f978, 0x1f97a,
  0x1f9cb, 0x1f9cd, 0x1f9ff, 0x1fa70, 0x1fa74, 0x1fa78, 0x1fa7a, 0x1fa80, 0x1fa86, 0x1fa90, 0x1faa8,
  0x1fab0, 0x1fab6, 0x1fac0, 0x1fac2, 0x1fad0, 0x1fad6, 0x20000, 0x2fffd, 0x30000, 0x3fffd,
];

/** Zero-width joiner: the scalar after it continues the cluster before it. */
const ZWJ = 0x200d;

/** Variation selector 16, which asks for the emoji presentation of a scalar. */
const VS16 = 0xfe0f;

/** One shared segmenter; constructing one costs more than using it. */
const GRAPHEMES: Intl$Segmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });

/**
 * Whether `code` falls inside a sorted, non-overlapping flat range table.
 *
 * The table is `[low, high, low, high, …]` rather than an array of pairs
 * because bisection over one flat array of numbers is both shorter to write
 * and the shape a JavaScript engine keeps unboxed.
 */
function inRanges(table: $ReadOnlyArray<number>, code: number): boolean {
  let low = 0;
  let high = table.length / 2 - 1;
  while (low <= high) {
    const middle = (low + high) >> 1;
    if (code < table[middle * 2]) {
      high = middle - 1;
    } else if (code > table[middle * 2 + 1]) {
      low = middle + 1;
    } else {
      return true;
    }
  }
  return false;
}

/**
 * The columns one Unicode scalar occupies.
 *
 * Control characters and combining marks occupy none, East Asian Wide and
 * Fullwidth characters and the default-emoji-presentation ranges occupy two,
 * and everything else occupies one.
 */
export function scalarWidth(code: number): number {
  if (code < 0x20 || (code >= 0x7f && code < 0xa0)) {
    return 0;
  }
  if (inRanges(ZERO_WIDTH, code)) {
    return 0;
  }
  return inRanges(WIDE, code) ? 2 : 1;
}

/**
 * The columns one grapheme cluster occupies.
 *
 * A cluster is as wide as its widest scalar rather than the sum of them: the
 * marks and joiners that make a cluster longer than one scalar are precisely
 * the ones that draw on top of what came before. The exception is the emoji
 * variation selector, which does not draw at all and instead widens the
 * narrow scalar in front of it — `❤` is one column and `❤️` is two, and they
 * differ by a code point that is invisible in every editor.
 */
export function graphemeWidth(cluster: string): number {
  let width = 0;
  let previousNarrow = false;
  for (const character of cluster) {
    const code = character.codePointAt(0) ?? 0;
    if (code === ZWJ) {
      previousNarrow = false;
      continue;
    }
    if (code === VS16) {
      if (previousNarrow) {
        width += 1;
        previousNarrow = false;
      }
      continue;
    }
    const scalar = scalarWidth(code);
    width = Math.max(width, scalar);
    previousNarrow = scalar === 1;
  }
  return width;
}

/** One grapheme cluster, with the columns it will occupy. */
export type Grapheme = {
  /** The cluster itself, as a string. */
  readonly text: string,
  /** How many columns it occupies: 0, 1, or 2. */
  readonly width: number,
};

/**
 * Split text into grapheme clusters, each carrying its width.
 *
 * Zero-width clusters are dropped rather than kept: a renderer that writes
 * them has to decide which cell they belong to, and the answer — "the one
 * before, which has already been written" — means the only correct handling is
 * to have merged them into that cluster, which `Intl.Segmenter` already did.
 * A lone combining mark with nothing to combine with is the one case this
 * loses, and losing it is better than reserving a column for something the
 * terminal will not advance the cursor over.
 */
export function graphemes(text: string): Array<Grapheme> {
  const out: Array<Grapheme> = [];
  for (const segment of GRAPHEMES.segment(text)) {
    const width = graphemeWidth(segment.segment);
    if (width > 0) {
      out.push({ text: segment.segment, width });
    }
  }
  return out;
}

/** The columns a whole string occupies. */
export function displayWidth(text: string): number {
  let width = 0;
  for (const segment of GRAPHEMES.segment(text)) {
    width += graphemeWidth(segment.segment);
  }
  return width;
}
