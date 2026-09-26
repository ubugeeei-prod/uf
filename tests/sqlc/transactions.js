// @flow
//
// One transaction scenario, run by each engine's suite against a real
// database behind each adapter: `npm/sql/sql.test.js` checks the
// statements `transactionOn` sends to a recording connection, and this checks
// that a real server agrees with what they mean.
//
// It is written against the `authors` example, whose `ListAuthors` orders by
// name, so the names below are listed in the order they sort.

import { SqlError } from "@uniflowed/sql";
import type { Queryable } from "@uniflowed/sql";
import { expect } from "@uniflowed/test";

/** Insert an author called `name`. */
export type Insert = (db: Queryable, name: string) => Promise<mixed>;

/** Every author's name, sorted. */
export type Names = (db: Queryable) => Promise<$ReadOnlyArray<string>>;

function transactionOf(db: Queryable): <T>(body: (tx: Queryable) => Promise<T>) => Promise<T> {
  const transaction = db.transaction;
  if (transaction === undefined) {
    throw new Error("this adapter was expected to open transactions");
  }
  return transaction;
}

async function failureOf(promise: Promise<mixed>): Promise<mixed> {
  try {
    await promise;
  } catch (error) {
    return error;
  }
  return "resolved";
}

/**
 * Commit, rollback, savepoints and a transaction used after it ended, on an
 * empty `authors` table.
 */
export async function transactionScenario(
  db: Queryable,
  insert: Insert,
  names: Names,
): Promise<void> {
  const transaction = transactionOf(db);

  // A body that resolves commits, and its value comes back.
  await expect(
    transaction(async (tx) => {
      await insert(tx, "committed");
      return "value";
    }),
  ).resolves.toBe("value");
  expect(await names(db)).toEqual(["committed"]);

  // A body that throws rolls back, and the caller gets the body's own error.
  const stop = new Error("stop");
  const thrown = await failureOf(
    transaction(async (tx) => {
      await insert(tx, "rolled back");
      throw stop;
    }),
  );
  expect(thrown).toBe(stop);
  expect(await names(db)).toEqual(["committed"]);

  // A transaction inside a transaction is a savepoint: its rollback undoes
  // only its own work, and the enclosing transaction carries on and commits.
  let leaked: Queryable | null = null;
  await transaction(async (tx) => {
    await insert(tx, "outer");
    const savepoint = transactionOf(tx);
    const inner = await failureOf(
      savepoint(async (sp) => {
        await insert(sp, "inner");
        throw stop;
      }),
    );
    expect(inner).toBe(stop);
    await savepoint(async (sp) => {
      await insert(sp, "kept");
    });
    expect(await names(tx)).toEqual(["committed", "kept", "outer"]);
    leaked = tx;
  });
  expect(await names(db)).toEqual(["committed", "kept", "outer"]);

  // A transaction's `Queryable` refuses statements once it has ended, rather
  // than running them outside the transaction without anyone noticing.
  const late = leaked === null ? "no transaction ran" : await failureOf(insert(leaked, "late"));
  expect(late instanceof SqlError ? late.failure.kind : late).toBe("closed");
  expect(await names(db)).toEqual(["committed", "kept", "outer"]);
}
