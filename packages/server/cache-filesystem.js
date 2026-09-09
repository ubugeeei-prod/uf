// @flow
//
// `@uniflowed/server/cache/filesystem`: one durable cache provider, on a disk.
//
// `./internal/cache-provider.js` is the seam and this is the implementation uf
// ships with it — deliberately the least interesting one that is actually
// correct, because `docs/red-lines.md` line 3 is about the seam being real and
// not about uf shipping the replacement. What proves the seam is that this file
// contains no cache logic at all: no staleness, no eviction policy beyond a
// bound on the directory, no notion of a fill. It reads and writes files. Every
// decision about *when* an entry may be used is in `./internal/cache-store.js`,
// where the contract is written down, and a Redis provider written by an
// adapter answers the same five methods with none of this.
//
// # A separate module, and separately exported
//
// `node:fs` is in here, so this cannot be `./cache.js`. `./fetch.js`'s header
// makes the rule: the application half "touches no filesystem, holds no Node
// types", which is what lets the same handler run in a worker. A worker's
// bundle must not acquire `node:fs` because a project turned a cache on, so the
// filesystem provider is a module a host imports on purpose — and a target
// without a filesystem simply never names it.
//
// # The layout, and why an entry is two files
//
// Under the directory, per entry:
//
//     <hash>.json   the record: key, timestamps, tags, path
//     <hash>.bin    the encoded value
//
// Two files rather than one, and the reason is `invalidateTag`. A tag
// invalidation has to find every entry carrying a name, and with one file per
// entry that means reading every entry — which for a route cache means reading
// every cached *document*, hundreds of kilobytes each, to look at a list of
// strings. Split, an invalidation reads only the small halves: a few hundred
// bytes per entry instead of a few hundred kilobytes.
//
// The alternative was a tag index, and it was rejected for a reason worth
// recording: an index is a second source of truth that several processes write
// at once, so it can disagree with the entries, and an invalidation that misses
// an entry because an index was stale is precisely the bug a cache must not
// have. Reading the directory is O(entries) and always right. If that becomes
// the bottleneck, the answer is a provider whose store has an index natively —
// which is what the seam is for.
//
// Write order is body then record, and remove order is record then body, so a
// reader that arrives mid-write sees an entry that does not exist yet rather
// than a record pointing at a body that is not there. What that leaves behind
// is an orphaned body, which costs disk and nothing else, and which the bound
// below takes out.
//
// # What bounds it
//
// A count of entries, swept coldest-first, exactly as `crates/uf_check`'s cache
// and the transform cache are bounded — and for the reason ubugeeei-prod/uf#218
// gives: nothing in a content-addressed cache ever *removes* an entry, so a
// directory only grows. Expired entries go first, because they cannot be served
// and their disk is free to reclaim; after them, the least recently modified.
//
// No timer, for the reason `./internal/cache-store.js` gives about the same
// question: a cache with a timer in it is a process that will not exit.

import { createHash } from "node:crypto";
import {
  mkdirSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  statSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";

import type { CacheProvider, DurableCacheEntry } from "./internal/cache-provider.js";

export type { CacheProvider, DurableCacheEntry } from "./internal/cache-provider.js";

/** How a filesystem provider is set up. */
export type FilesystemCacheOptions = {|
  /**
   * Where entries go.
   *
   * Required, and named by the host rather than defaulted here, because the
   * right answer is a fact about the deployment and nothing in this module
   * knows it: a container wants a mounted volume that outlives the container, a
   * Lambda has only `/tmp` and only for the life of one execution environment,
   * and `uf start` on a box wants somewhere under the project. A default would
   * be right for one of those and quietly wrong for the others.
   */
  readonly directory: string,
  /**
   * Most entries kept. Defaults to 1024, the same as the store's memory bound.
   *
   * The same number for a different resource, and they are not the same limit:
   * this one is shared by every process reading the directory and survives all
   * of them, where `maxEntries` bounds one process's heap.
   */
  readonly maxEntries?: number,
|};

/** One durable cache provider, keeping entries in a directory. */
export type FilesystemCacheProvider = {
  ...CacheProvider,
  /** The directory entries are in, resolved. */
  readonly directory: string,
  ...
};

/**
 * What a record file holds: an entry without its value.
 *
 * Spelled out rather than derived from `DurableCacheEntry` because it is
 * deliberately *not* one — the value lives in the other file, and a type that
 * carried an always-empty `value` would invite somebody to read it.
 */
type Record = {|
  /** The durable key, for a person reading the directory. Never matched on. */
  readonly key: string,
  readonly storedAt: number,
  readonly revalidateAt: number,
  readonly expiresAt: number,
  readonly tags: $ReadOnlyArray<string>,
  readonly path: string | null,
|};

/**
 * A provider that keeps entries in `directory`.
 *
 * The directory is created here, and a directory that cannot be created is a
 * failure of *this call* rather than of the first request. That is the one
 * place this module is strict, and it is the opposite of what
 * `@uniflowed/host`'s transform cache does with the same failure — it writes
 * tolerantly and a read-only checkout simply runs slower. The difference is who
 * asked. A transform cache is uf's own idea and nobody opted into it; a durable
 * route cache is something a project turned on and expects, so a deployment
 * whose disk it cannot use should fail where it is wired instead of answering
 * every request from an empty cache while `x-uf-cache` says `MISS` forever.
 */
export function createFilesystemCache(options: FilesystemCacheOptions): FilesystemCacheProvider {
  const directory = path.resolve(options.directory);
  const maxEntries = options.maxEntries ?? 1024;
  if (!Number.isInteger(maxEntries) || maxEntries < 1) {
    throw new RangeError(
      `@uniflowed/server: maxEntries is ${String(maxEntries)}; a durable store that can hold ` +
        "no entries is a directory that only costs.",
    );
  }
  try {
    mkdirSync(directory, { recursive: true });
  } catch (error) {
    throw new Error(
      `@uniflowed/server: the durable cache directory ${directory} could not be created. ` +
        "A project that asked for a cache that survives a restart should be told here " +
        "rather than serve every request from an empty one.",
      { cause: error },
    );
  }

  // What this process believes is in the directory, so that an ordinary write
  // is one write and not a `readdir`. Deliberately a belief and not a fact:
  // other processes are writing the same directory, so it drifts, and every
  // sweep replaces it with a count that was true a moment ago. Drift costs a
  // sweep that happens slightly early or slightly late, which is the cheapest
  // thing here to be wrong about.
  let believed: number | null = null;

  const recordFile = (key: string) => path.join(directory, `${digest(key)}.json`);
  const bodyFile = (key: string) => path.join(directory, `${digest(key)}.bin`);

  const provider: FilesystemCacheProvider = {
    name: "filesystem",
    directory,

    async read(key: string): Promise<DurableCacheEntry | null> {
      const record = readRecord(recordFile(key));
      if (record == null) {
        return null;
      }
      let value: string;
      try {
        value = readFileSync(bodyFile(key), "utf8");
      } catch {
        // A record whose body is gone: a half-finished remove, or a sweep that
        // took the body of an entry written at the same moment. Not an error —
        // the store reads it as a miss and renders, which is right.
        return null;
      }
      return {
        value,
        storedAt: record.storedAt,
        revalidateAt: record.revalidateAt,
        expiresAt: record.expiresAt,
        tags: record.tags,
        path: record.path,
      };
    },

    async write(key: string, entry: DurableCacheEntry): Promise<void> {
      const record: Record = {
        key,
        storedAt: entry.storedAt,
        revalidateAt: entry.revalidateAt,
        expiresAt: entry.expiresAt,
        tags: entry.tags,
        path: entry.path,
      };
      // Body first: a reader that arrives between the two finds no record and
      // renders, where the other order would find a record naming a body that
      // is not there yet.
      writeAtomically(bodyFile(key), entry.value);
      writeAtomically(recordFile(key), JSON.stringify(record));
      believed = (believed ?? countEntries(directory)) + 1;
      if (believed > maxEntries) {
        believed = sweep(directory, maxEntries, Date.now());
      }
    },

    async remove(key: string): Promise<void> {
      // Record first: the entry stops being reachable at the first unlink, so
      // there is no moment where it is half-removed and still servable.
      const existed = discard(recordFile(key));
      discard(bodyFile(key));
      if (existed && believed != null) {
        believed -= 1;
      }
    },

    async invalidateTag(tag: string): Promise<number> {
      const dropped = expireWhere(directory, (record) => record.tags.includes(tag));
      if (dropped > 0) believed = null;
      return dropped;
    },

    async invalidatePath(target: string): Promise<number> {
      const dropped = expireWhere(directory, (record) => record.path === target);
      if (dropped > 0) believed = null;
      return dropped;
    },

    async clear(): Promise<void> {
      for (const name of list(directory)) {
        discard(path.join(directory, name));
      }
      believed = 0;
    },
  };
  return provider;
}

/**
 * The filename a durable key gets.
 *
 * The key is already exact — `./internal/cache-key.js` builds it so that two
 * keys cannot collide and one key cannot spell itself twice — but it is a JSON
 * array containing slashes, quotes and anything a URL may contain, and that is
 * not a filename on any filesystem. So it is hashed, and the key it came from
 * is written *inside* the record: a person looking at a directory of hex names
 * can still find out what each one is about, which is the whole reason the
 * record carries a field nothing matches on.
 */
function digest(key: string): string {
  return createHash("sha256").update(key).digest("hex");
}

/** The record at `file`, or `null` for anything that is not one. */
function readRecord(file: string): Record | null {
  let parsed: mixed;
  try {
    parsed = JSON.parse(readFileSync(file, "utf8"));
  } catch {
    // Absent, or half-written by a process that died between the rename and
    // the flush. Both read as "there is no entry here", which is true.
    return null;
  }
  if (parsed == null || typeof parsed !== "object") return null;
  const record: $FlowFixMe = parsed;
  if (typeof record.expiresAt !== "number" || typeof record.revalidateAt !== "number") return null;
  if (!Array.isArray(record.tags)) return null;
  return {
    key: typeof record.key === "string" ? record.key : "",
    storedAt: typeof record.storedAt === "number" ? record.storedAt : 0,
    revalidateAt: record.revalidateAt,
    expiresAt: record.expiresAt,
    tags: record.tags.filter((tag: mixed) => typeof tag === "string"),
    path: typeof record.path === "string" ? record.path : null,
  };
}

/** Everything in `directory`, or nothing when it has gone. */
function list(directory: string): Array<string> {
  try {
    return readdirSync(directory);
  } catch {
    return [];
  }
}

/** How many entries the directory holds, by counting records. */
function countEntries(directory: string): number {
  return list(directory).filter((name) => name.endsWith(".json")).length;
}

/** Drop every entry `matches` describes, counting them. */
function expireWhere(directory: string, matches: (record: Record) => boolean): number {
  let dropped = 0;
  for (const name of list(directory)) {
    if (!name.endsWith(".json")) continue;
    const record = readRecord(path.join(directory, name));
    if (record == null || !matches(record)) continue;
    discard(path.join(directory, name));
    discard(path.join(directory, `${stemOf(name)}.bin`));
    dropped += 1;
  }
  return dropped;
}

/**
 * Bring the directory back under `maxEntries`, answering what is left.
 *
 * Expired entries first — they cannot be served, so removing them costs
 * nothing — and then the least recently written, which is the same order the
 * in-memory store evicts in and is picked for the same reason: what nothing has
 * wanted for longest is the cheapest thing to make somebody render again.
 *
 * `mtime` rather than a recorded read time, because reading an entry here does
 * not write anything and making it write would turn every cache hit into a
 * disk write. The consequence is that this is least-recently-*written* rather
 * than least-recently-used, which for entries that all have a stated lifetime
 * is close enough to be the same list in a different order.
 */
function sweep(directory: string, maxEntries: number, at: number): number {
  const records: Array<{| name: string, expiresAt: number, mtime: number |}> = [];
  for (const name of list(directory)) {
    if (!name.endsWith(".json")) continue;
    const file = path.join(directory, name);
    const record = readRecord(file);
    let mtime = 0;
    try {
      mtime = statSync(file).mtimeMs;
    } catch {
      continue;
    }
    records.push({ name, expiresAt: record?.expiresAt ?? 0, mtime });
  }

  const doomed = records
    .filter((entry) => entry.expiresAt <= at)
    .concat(
      records
        .filter((entry) => entry.expiresAt > at)
        .sort((left, right) => left.mtime - right.mtime),
    );
  const live = new Set(records.map((entry) => stemOf(entry.name)));
  let kept = records.length;
  for (const entry of doomed) {
    if (kept <= maxEntries) break;
    const stem = stemOf(entry.name);
    discard(path.join(directory, entry.name));
    live.delete(stem);
    kept -= 1;
  }

  // Every body with no record above it: the ones this sweep just orphaned, and
  // the ones left behind by a removal interrupted between its two unlinks.
  // Cleaned here rather than never, because nothing will ever look at them
  // again and nothing else walks this directory.
  for (const name of list(directory)) {
    if (!name.endsWith(".bin")) continue;
    if (!live.has(name.slice(0, -".bin".length))) {
      discard(path.join(directory, name));
    }
  }
  return kept;
}

/** The hex name a `<hash>.json` is about. */
function stemOf(name: string): string {
  return name.slice(0, -".json".length);
}

/** Remove `file`. Answers whether there was one. */
function discard(file: string): boolean {
  try {
    unlinkSync(file);
    return true;
  } catch {
    try {
      // A directory somebody put in the cache directory by hand. Removing it is
      // not this module's business beyond not tripping over it.
      rmSync(file, { recursive: false });
      return true;
    } catch {
      return false;
    }
  }
}

/**
 * Write `contents` to `file` so a concurrent reader never sees half of it.
 *
 * The same technique and the same reasoning as
 * `@uniflowed/host/write-atomically` — a truncate-then-write is observable in
 * its middle, and ubugeeei-prod/uf#240 is what that costs — and a separate copy
 * of it because `@uniflowed/server` may not depend on `@uniflowed/host`: the
 * host package is the toolchain, and this one is what a deployment links.
 *
 * Not tolerant. A durable cache that cannot write is the thing a project asked
 * for not happening, so the failure goes back to `./internal/cache-store.js`,
 * which reports it through `onError` and answers the request from the render.
 */
function writeAtomically(file: string, contents: string): void {
  const temporary = `${file}.${process.pid}.${Math.random().toString(36).slice(2)}`;
  try {
    writeFileSync(temporary, contents);
    renameSync(temporary, file);
  } catch (error) {
    discard(temporary);
    throw error;
  }
}
