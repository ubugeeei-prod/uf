// @flow
//
// `@uniflowed/orm`.

import type {
  NativeHandle,
  NativeHandleInvariant,
} from "./internal/native-runtime.js";
import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/orm";

export opaque type Table<Row: { +[string]: mixed }> = NativeHandleInvariant<
  "@uniflowed/core/orm#Table",
  Row,
>;
export opaque type Column<T> = NativeHandleInvariant<
  "@uniflowed/core/orm#Column",
  T,
>;
export opaque type Query<Row: { +[string]: mixed }> = NativeHandleInvariant<
  "@uniflowed/core/orm#Query",
  Row,
>;
export opaque type Migration = NativeHandle<"@uniflowed/core/orm#Migration">;

export type ColumnType =
  | "text"
  | "int64"
  | "boolean"
  | "json"
  | "timestamp"
  | "uuid";

export type OrmContract = {
  +driver: "sqlite" | "postgres" | "mysql",
  +preparedByDefault: true,
  +generatedFlowTypes: true,
  +parameterizedQueriesOnly: true,
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

export function defineTable<Row: { +[string]: mixed }>(
  name: string,
  columns: { +[string]: Column<mixed> },
): Table<Row> {
  return nativeRuntimeRequired(MODULE, "defineTable");
}

export function relation<
  From: { +[string]: mixed },
  To: { +[string]: mixed },
>(from: Table<From>, to: Table<To>): void {
  return nativeRuntimeRequired(MODULE, "relation");
}

export function query<Row: { +[string]: mixed }>(
  table: Table<Row>,
): Query<Row> {
  return nativeRuntimeRequired(MODULE, "query");
}

export function whereEq<Row: { +[string]: mixed }, Value>(
  query: Query<Row>,
  column: Column<Value>,
  value: Value,
): Query<Row> {
  return nativeRuntimeRequired(MODULE, "whereEq");
}

export function limit<Row: { +[string]: mixed }>(
  query: Query<Row>,
  count: number,
): Query<Row> {
  return nativeRuntimeRequired(MODULE, "limit");
}

export function migration(name: string): Migration {
  return nativeRuntimeRequired(MODULE, "migration");
}

export const db: {
  query<Row: { +[string]: mixed }>(
    query: Query<Row>,
  ): Promise<$ReadOnlyArray<Row>>,
} = {
  query<Row: { +[string]: mixed }>(
    query: Query<Row>,
  ): Promise<$ReadOnlyArray<Row>> {
    return nativeRuntimeRequired(MODULE, "db.query");
  },
};

export function contract(): OrmContract {
  return nativeRuntimeRequired(MODULE, "contract");
}
