// @flow
//
// The version the extension is published as, from the version of uf it ships
// with.
//
// # Why it is not simply uf's version
//
// The Visual Studio Marketplace takes `major.minor.patch` and nothing else:
// `vsce publish` refuses `0.0.0-alpha.46` outright ("The VS Marketplace
// doesn't support prerelease versions"). A pre-release is instead an ordinary
// version with a pre-release flag in the package, and VS Code updates to the
// highest number it can see. So uf's versions are mapped onto plain versions
// that sort in the same order uf's do, prerelease before release:
//
//   uf                  extension
//   0.0.0-alpha.46      0.0.46       pre-release
//   1.2.0-beta.3        1.2.1003     pre-release
//   1.2.0-rc.1          1.2.2001     pre-release
//   1.2.0               1.2.9999
//   1.2.1               1.2.19999
//
// The patch is `patch * 10000`, plus the prerelease's place in its patch
// (`alpha` from 0, `beta` from 1000, `rc` from 2000) or 9999 for the release
// itself, which sorts after all of them. Major and minor are uf's own. Any
// other shape — another prerelease word, a thousandth alpha — is refused
// rather than guessed at, because a number that sorts wrong is published for
// good: the Marketplace never takes a version back.
//
// Open VSX is sent the same file, so both registries agree on the number.
//
// This file is not part of the extension (`.vscodeignore` leaves `release/`
// out of the package); `.github/workflows/editors.yml` runs it, and
// `tests/library/vscode-extension.test.js` tests it.

"use strict";

/*::
export type Published = {
  // The `version` written into package.json before packaging.
  version: string,
  // Whether it is packaged and published with `--pre-release`.
  preRelease: boolean,
};
*/

const PRERELEASE_WORDS = ["alpha", "beta", "rc"];
const PER_WORD = 1000;
const PER_PATCH = 10000;
const RELEASE = PER_PATCH - 1;

const SHAPE = /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([a-z]+)\.(0|[1-9][0-9]*))?$/;

/**
 * The extension version for uf version `uf`, or an error saying why there is
 * none.
 */
function marketplaceVersion(uf /*: string */) /*: Published */ {
  const match = SHAPE.exec(uf);
  if (match == null) {
    throw new Error(
      `uf ${uf} has no extension version: expected major.minor.patch, optionally ` +
        "followed by -alpha.N, -beta.N or -rc.N",
    );
  }
  const [, major, minor, patchText, word, numberText] = match;
  const patch = Number(patchText);
  if (word == null) {
    return { version: `${major}.${minor}.${patch * PER_PATCH + RELEASE}`, preRelease: false };
  }
  const place = PRERELEASE_WORDS.indexOf(word);
  if (place < 0) {
    throw new Error(
      `uf ${uf} has no extension version: the prerelease must be alpha, beta or rc, not ${word}`,
    );
  }
  const number = Number(numberText);
  if (number >= PER_WORD) {
    throw new Error(
      `uf ${uf} has no extension version: a prerelease number must be below ${PER_WORD}`,
    );
  }
  return {
    version: `${major}.${minor}.${patch * PER_PATCH + place * PER_WORD + number}`,
    preRelease: true,
  };
}

module.exports = { marketplaceVersion };

// `node editors/vscode/release/version.js 0.0.0-alpha.46` prints the two lines
// the workflow appends to `$GITHUB_OUTPUT`.
if (require.main === module) {
  const published = marketplaceVersion(process.argv[2] ?? "");
  process.stdout.write(
    `marketplace_version=${published.version}\npre_release=${String(published.preRelease)}\n`,
  );
}
