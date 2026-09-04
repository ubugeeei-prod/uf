// @flow
//
// `@uniflowed/orm`.

import type { NativeHandle, NativeHandleInvariant } from "@uniflowed/core/native";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/orm";

export opaque type Table<Row extends { readonly [string]: mixed }> = NativeHandleInvariant<
  "@uniflowed/core/orm#Table",
  Row,
>;
export opaque type Column<T> = NativeHandleInvariant<"@uniflowed/core/orm#Column", T>;
export opaque type Query<Row extends { readonly [string]: mixed }> = NativeHandleInvariant<
  "@uniflowed/core/orm#Query",
  Row,
>;
export opaque type Migration = NativeHandle<"@uniflowed/core/orm#Migration">;

export type ColumnType = "text" | "int64" | "boolean" | "json" | "timestamp" | "uuid";

export type OrmContract = {
  readonly driver: "sqlite" | "postgres" | "mysql",
  readonly preparedByDefault: true,
  readonly generatedFlowTypes: true,
  readonly parameterizedQueriesOnly: true,
};

export function column<T>(name: string, ty: ColumnType): Column<T> {
  return nativeRuntimeRequired(MODULE, "column");
}

export function primaryKey<T>(column: Column<T>): Column<T> {
  return nativeRuntimeRequired(MODULE, "primaryKey");
}

export function nullable<T>(column: Column<T>): Column<?T> {
  return nativeRuntimeRequired(MODULE, "nullable");
}

export function defineTable<Row extends { readonly [string]: mixed }>(
  name: string,
  columns: { readonly [string]: Column<mixed> },
): Table<Row> {
  return nativeRuntimeRequired(MODULE, "defineTable");
}

export function relation<
  From extends { readonly [string]: mixed },
  To extends { readonly [string]: mixed },
>(from: Table<From>, to: Table<To>): void {
  return nativeRuntimeRequired(MODULE, "relation");
}

export function query<Row extends { readonly [string]: mixed }>(table: Table<Row>): Query<Row> {
  return nativeRuntimeRequired(MODULE, "query");
}

export function whereEq<Row extends { readonly [string]: mixed }, Value>(
  query: Query<Row>,
  column: Column<Value>,
  value: Value,
): Query<Row> {
  return nativeRuntimeRequired(MODULE, "whereEq");
}

export function limit<Row extends { readonly [string]: mixed }>(
  query: Query<Row>,
  count: number,
): Query<Row> {
  return nativeRuntimeRequired(MODULE, "limit");
}

export function migration(name: string): Migration {
  return nativeRuntimeRequired(MODULE, "migration");
}

export const db: {
  query<Row extends { readonly [string]: mixed }>(query: Query<Row>): Promise<$ReadOnlyArray<Row>>,
} = {
  query<Row extends { readonly [string]: mixed }>(query: Query<Row>): Promise<$ReadOnlyArray<Row>> {
    return nativeRuntimeRequired(MODULE, "db.query");
  },
};

export function contract(): OrmContract {
  return nativeRuntimeRequired(MODULE, "contract");
}
