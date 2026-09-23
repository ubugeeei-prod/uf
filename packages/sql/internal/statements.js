// @flow
//
// A bounded cache of prepared statements, for the SQLite adapters.
//
// Generated code runs the same few dozen statements over and over, and
// preparing one costs a parse and a plan. The texts are constants in the
// generated modules, so the cache's key space is the application's queries —
// except for `sqlc.slice` and `:copyfrom`, whose text grows with the input.
// The bound keeps those from growing it without limit; the least recently
// used statement goes first.

export const DEFAULT_STATEMENT_CACHE = 256;

export function statementCache<S>(
  prepare: (text: string) => S,
  limit: number,
): (text: string) => S {
  const cache: Map<string, S> = new Map();
  return (text) => {
    const hit = cache.get(text);
    if (hit !== undefined) {
      // Re-insert, so iteration order is least recently used first.
      cache.delete(text);
      cache.set(text, hit);
      return hit;
    }
    const statement = prepare(text);
    if (limit > 0) {
      if (cache.size >= limit) {
        const oldest = cache.keys().next();
        if (!oldest.done) {
          cache.delete(oldest.value);
        }
      }
      cache.set(text, statement);
    }
    return statement;
  };
}

/** A driver's row count or row id, which may be a `number` or a `bigint`, as a `number`. */
export function count(value: number | bigint): number {
  return typeof value === "bigint" ? Number(value) : value;
}

/** A driver's last row id as a `bigint`, or `null` when it reported none. */
export function rowId(value: mixed): bigint | null {
  if (typeof value === "bigint") {
    return value;
  }
  if (typeof value === "number" && Number.isSafeInteger(value)) {
    return BigInt(value);
  }
  return null;
}
