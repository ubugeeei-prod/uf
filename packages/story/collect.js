// @flow
//
// `@uniflowed/story/collect`: which files are stories, and what is in them.
//
// A story file is `_uf.story.js`, beside the component it describes.
//
//   src/components/Button.js
//   src/components/_uf.story.js
//
// # The name is the repository's own grammar, not a second one
//
// uf already reserves `_uf.<role>[.<variant>].js` for the files the framework
// gives meaning to, and `crates/uf_router/src/reserved.rs` is its single
// source of truth: `uf create` generates those names, the router looks for
// them, and `uf lint`'s `router/reserved-files` rejects the ones that do not
// fit. A story file is exactly such a file — a name uf assigns meaning to, in
// a project's own source tree — so it is spelled `_uf.story.js` and not
// `*.stories.js`.
//
// The variants are the same vocabulary for the same reason, and the rule
// about them is the router's too: only the default variant is the thing the
// runner renders. `_uf.story.native.js` is a companion for a React Native
// build, the way `_uf.page.native.js` is, and [`findStoryFiles`] leaves it
// alone until there is a renderer that could mount it.
//
// **`story` is not yet one of the roles the Rust grammar defines.** Adding it
// is one arm in `ReservedRole` in `crates/uf_router/src/reserved.rs`, and
// this package cannot make that change from JavaScript. Until it lands,
// `uf lint` reports `router/reserved-files` on every `_uf.story.js` — the
// name is right and the linter has not been told. `index.js` lists it under
// **Readiness** rather than leaving it to be discovered by whoever writes the
// first story file.
//
// # What makes a file a story file is the name, and what makes it valid is
// the export
//
// Discovery is by name alone: a walk that had to read every file to find out
// whether it declared stories would be a parse of the whole tree. Loading is
// where a file is judged, and a `_uf.story.js` that exports no story set is
// an error rather than an empty result — the name is a claim, and a file that
// does not honour it is a mistake somebody made, not a fact about the
// project.
//
// Every export is considered, not a conventional name and not the default
// export. A file may hold the stories of two components, and naming them
// after what they are is better than naming one of them `default`. uf's
// `react/no-default-export-component` lint rule points the same way.
//
// # Why this walk is in JavaScript, and where it stops being acceptable
//
// uf's rule is that repository-wide file discovery belongs in Rust, and it is
// the right rule: `uf test` and the router both discover natively, and both
// are on the hot path of every keystroke in watch mode. This walk is not that
// yet. It is bounded work a test or a script asks for once — one `readdir`
// per directory, no file read, no parse — and it exists because there is no
// `uf story` command to discover natively *for* it.
//
// When one lands, the walk belongs in `uf_router`-shaped Rust beside the
// route discovery it mirrors, with this left as the loader the host calls
// with the paths it was given. [`loadStoryFile`] and [`indexStories`] are
// already separate from [`findStoryFiles`] for that reason: replacing the
// walk does not touch them.

import { readdir } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

import type { Story, StorySet } from "./story.js";
import { isStorySet } from "./story.js";

/** The role segment a story file carries. */
export const STORY_ROLE: "story" = "story";

/** The name of a story file with no variant: the one the runner renders. */
export const STORY_FILE: "_uf.story.js" = "_uf.story.js";

/**
 * Which build a story file applies to.
 *
 * The router's vocabulary, exactly. `"default"` has no segment in the name.
 */
export type StoryVariant = "default" | "native" | "ios" | "android" | "web" | "test";

/** The variants, in the order `uf_router::ReservedVariant` declares them. */
const VARIANTS: $ReadOnlyArray<StoryVariant> = ["native", "ios", "android", "web", "test"];

/** Directory names the walk never descends into. */
const IGNORED: $ReadOnlyArray<string> = [
  "node_modules",
  ".git",
  ".uf",
  "dist",
  "build",
  "target",
  "coverage",
];

/**
 * The variant of `fileName` as a story file, or `null` if it is not one.
 *
 * Takes a file name, not a path, for the reason the Rust classifier does:
 * a caller with a path can normalise it two ways and get two answers.
 */
export function classifyStoryFile(fileName: string): StoryVariant | null {
  if (!fileName.startsWith("_uf.") || !fileName.endsWith(".js")) {
    return null;
  }
  const segments = fileName.slice("_uf.".length, -".js".length).split(".");
  if (segments[0] !== STORY_ROLE) {
    return null;
  }
  if (segments.length === 1) {
    return "default";
  }
  if (segments.length > 2) {
    // `_uf.story.native.test.js`: one variant, not a stack of them.
    return null;
  }
  const variant = VARIANTS.find((each) => each === segments[1]);
  return variant ?? null;
}

/**
 * Whether `fileName` is the story file a renderer mounts.
 *
 * The platform variants are companions to a story, never stories of their
 * own — the same distinction `ReservedVariant::is_route_entry` draws.
 */
export function isStoryEntry(fileName: string): boolean {
  return classifyStoryFile(fileName) === "default";
}

/** What to leave out of a walk. */
export type FindOptions = {|
  /** Directory names to skip, replacing the defaults. */
  readonly ignore?: $ReadOnlyArray<string>,
  /**
   * How deep to descend below `root`. Defaults to 32.
   *
   * A bound rather than a promise not to recurse: a directory tree is
   * untrusted input the moment a generated or vendored directory is in it,
   * and an unbounded walk is an unbounded stack.
   */
  readonly maxDepth?: number,
|};

/**
 * Every story file under `root`, as absolute paths, in a stable order.
 *
 * Sorted, so a catalogue, a report and a set of baselines come out in the
 * same order on every machine.
 *
 * Symbolic links are not followed. That falls out of asking `readdir` for
 * directory entries rather than stating it as a rule: an entry describes the
 * link itself, so a link is neither `isDirectory()` nor `isFile()` and the
 * walk passes it by. It has to be that way round — a link to a parent is a
 * walk that does not terminate, and a link into `node_modules` is somebody
 * else's story file.
 */
export async function findStoryFiles(root: string, options?: FindOptions): Promise<Array<string>> {
  const ignore = new Set(options?.ignore ?? IGNORED);
  const maxDepth = options?.maxDepth ?? 32;
  const found: Array<string> = [];

  const walk = async (directory: string, depth: number): Promise<void> => {
    if (depth > maxDepth) {
      return;
    }
    const entries = (await readdir(directory, { withFileTypes: true })).map((entry) => ({
      entry,
      // Flow's Node library definition types `Dirent.name` as `string |
      // Buffer`, because `readdir` can be asked for buffers. This call does
      // not ask; the conversion is what says so, rather than a cast that
      // would be wrong the day somebody adds an encoding.
      name: typeof entry.name === "string" ? entry.name : entry.name.toString("utf8"),
    }));
    entries.sort((left, right) => (left.name < right.name ? -1 : 1));

    for (const { entry, name } of entries) {
      if (entry.isDirectory()) {
        if (!ignore.has(name)) {
          await walk(path.join(directory, name), depth + 1);
        }
      } else if (entry.isFile() && isStoryEntry(name)) {
        found.push(path.join(directory, name));
      }
    }
  };

  await walk(path.resolve(root), 0);
  return found;
}

/**
 * Import `file` and return every story set it exports.
 *
 * Throws when it exports none. See the module docs: the name is a claim.
 *
 * The import is by `file:` URL rather than by path, because a bare path is
 * resolved against the *importer* on every host uf supports, and this module
 * is not where the story file lives.
 */
export async function loadStoryFile(file: string): Promise<Array<StorySet>> {
  const absolute = path.resolve(file);
  const module = await import(pathToFileURL(absolute).href);
  const sets: Array<StorySet> = [];
  for (const name of Object.keys(module)) {
    const value = module[name];
    if (isStorySet(value)) {
      sets.push(value);
    }
  }
  if (sets.length === 0) {
    throw new Error(`@uniflowed/story: ${absolute} exports no story set`);
  }
  return sets;
}

/** Every story a project declares, and how to reach one. */
export type StoryIndex = {|
  /** The files the sets came from, in walk order. */
  readonly files: $ReadOnlyArray<string>,
  /** Every set, in the order its file was found. */
  readonly sets: $ReadOnlyArray<StorySet>,
  /** Every story in every set, flattened, in declaration order. */
  readonly stories: $ReadOnlyArray<Story>,
  /** The story with this id, or `undefined`. */
  readonly get: (id: string) => Story | void,
|};

/**
 * Build an index from files that have already been found.
 *
 * Separate from [`collectStories`] so that a host which discovered the files
 * some other way — natively, from a watcher, from a changed-files list — can
 * still build the same index.
 *
 * Two stories with one id is an error, and it names both files. An id is what
 * a failing job prints and what a visual baseline is filed under, so a
 * collision does not produce a confusing result: it produces the *wrong* one,
 * silently, for whichever of the two was written second.
 */
export async function indexStories(files: $ReadOnlyArray<string>): Promise<StoryIndex> {
  const sets: Array<StorySet> = [];
  const stories: Array<Story> = [];
  const byId: Map<string, Story> = new Map();
  const sources: Map<string, string> = new Map();

  for (const file of files) {
    for (const set of await loadStoryFile(file)) {
      sets.push(set);
      for (const story of set.stories) {
        const clash = sources.get(story.id);
        if (clash != null) {
          throw new Error(
            `@uniflowed/story: two stories share the id ${story.id}\n` +
              `  ${clash}\n  ${file}\n` +
              "Give one of them a different title or name.",
          );
        }
        sources.set(story.id, file);
        byId.set(story.id, story);
        stories.push(story);
      }
    }
  }

  return {
    files: [...files],
    sets,
    stories,
    get: (id: string) => byId.get(id),
  };
}

/** Find every story file under `root` and index what they declare. */
export async function collectStories(root: string, options?: FindOptions): Promise<StoryIndex> {
  return indexStories(await findStoryFiles(root, options));
}
