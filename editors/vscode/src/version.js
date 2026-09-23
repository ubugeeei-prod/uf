// @flow
//
// Whether the `uf` this extension found is one it can talk to.
//
// # Where the version comes from
//
// From the server itself: `initialize` answers with
// `serverInfo: { name: "uf-lsp", version }`, and `version` is the version of
// the `uf` binary that answered — the crate version it was compiled with. So
// no second process is started to ask `uf --version`, and the version checked
// is the version of the process actually serving this folder, not of whatever
// `uf` a shell would find.
//
// # What "too old" means
//
// `MINIMUM_UF` is the oldest release whose server answers everything this
// extension wires up: diagnostics, formatting, quick fixes, fix-all, hover,
// definitions and completion. An older `uf` still starts — the extension does
// not refuse it, since formatting and lint findings from an old server are
// better than none — but the user is told, once per start, which version they
// have and what to do about it.
//
// A version this file cannot read (a local build that reports something odd)
// is not called old. Guessing wrong in that direction nags someone who is
// running a build newer than any release.

"use strict";

// uf 0.1.0 is the first release after the 0.0.0-alpha series, and the first
// whose server answers hover, definitions and completion from Flow's
// inference.
const MINIMUM_UF = "0.1.0";

/*::
export type Version = {
  readonly major: number,
  readonly minor: number,
  readonly patch: number,
  // `null` for a release, which sorts after every prerelease of its version.
  readonly prerelease: string | null,
};

export type VersionCheck =
  | { readonly kind: "supported", readonly version: string }
  | { readonly kind: "old", readonly version: string, readonly message: string }
  | { readonly kind: "unknown", readonly version: string | null };
*/

const SHAPE =
  /^v?(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z.-]+))?(?:\+.*)?$/;

/**
 * A version string as uf writes it, or `null` when it is not one.
 */
function parseVersion(text /*: string */) /*: Version | null */ {
  const match = SHAPE.exec(text.trim());
  if (match == null) {
    return null;
  }
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
    prerelease: match[4] == null ? null : match[4],
  };
}

/**
 * Compare two prerelease identifiers the way semver does: dot-separated
 * parts, numeric parts numerically and before alphanumeric ones.
 */
function comparePrerelease(left /*: string */, right /*: string */) /*: number */ {
  const a = left.split(".");
  const b = right.split(".");
  for (let index = 0; index < Math.max(a.length, b.length); index += 1) {
    if (index >= a.length) {
      return -1;
    }
    if (index >= b.length) {
      return 1;
    }
    const x = a[index];
    const y = b[index];
    const xNumeric = /^[0-9]+$/.test(x);
    const yNumeric = /^[0-9]+$/.test(y);
    if (xNumeric && yNumeric) {
      const difference = Number(x) - Number(y);
      if (difference !== 0) {
        return difference < 0 ? -1 : 1;
      }
    } else if (xNumeric !== yNumeric) {
      return xNumeric ? -1 : 1;
    } else if (x !== y) {
      return x < y ? -1 : 1;
    }
  }
  return 0;
}

/**
 * Negative when `left` is older than `right`, positive when newer, zero when
 * they are the same release.
 */
function compareVersions(left /*: Version */, right /*: Version */) /*: number */ {
  const pairs = [
    [left.major, right.major],
    [left.minor, right.minor],
    [left.patch, right.patch],
  ];
  for (const [x, y] of pairs) {
    if (x !== y) {
      return x < y ? -1 : 1;
    }
  }
  if (left.prerelease === right.prerelease) {
    return 0;
  }
  if (left.prerelease == null) {
    return 1;
  }
  if (right.prerelease == null) {
    return -1;
  }
  return comparePrerelease(left.prerelease, right.prerelease);
}

/**
 * What to make of the version a server reported.
 */
function checkVersion(
  reported /*: string | null | void */,
  minimum /*: string */ = MINIMUM_UF,
) /*: VersionCheck */ {
  if (reported == null || reported === "") {
    return { kind: "unknown", version: null };
  }
  const version = parseVersion(reported);
  const floor = parseVersion(minimum);
  if (version == null || floor == null) {
    return { kind: "unknown", version: reported };
  }
  if (compareVersions(version, floor) >= 0) {
    return { kind: "supported", version: reported };
  }
  return {
    kind: "old",
    version: reported,
    message:
      `uf: this project runs uf ${reported}, older than ${minimum}, the oldest this extension ` +
      "supports. Diagnostics and formatting still work; hover, definitions and completion " +
      "may not. Update the project's uf, or run `uf self-update` for a global one.",
  };
}

module.exports = { MINIMUM_UF, checkVersion, compareVersions, parseVersion };
