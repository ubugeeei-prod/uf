// @flow
//
// `@uniflowed/std/csv`: Go's `encoding/csv`, as pure Flow functions.
//
// CSV is the format people keep reimplementing with `line.split(",")`, and the
// bugs are always the same: a comma inside a quoted field, a quote escaped as
// `""`, or a newline inside a quoted field. Those three cases are not edge
// cases; they are why CSV exists rather than "comma separated strings".
//
// This module keeps the surface intentionally small. [`parse`] turns text into
// rows of strings, [`stringify`] turns rows back into text, and both stay
// runtime-agnostic: no `node:` streams, no `Buffer`, no locale.
//
// # Shape checks
//
// Go's reader infers the number of fields from the first record by default, and
// that is the useful default here too: a missing trailing column is more often
// a malformed export than a valid sparse row. Pass `fieldsPerRecord: "variable"`
// when ragged rows are intentional, or a number when the caller already knows
// the schema.

export type ParseOptions = {
  /** Field separator. Defaults to `,`. Must be one character. */
  readonly comma?: string,
  /**
   * Field-count policy. Defaults to `"infer"`, using the first record's width.
   */
  readonly fieldsPerRecord?: number | "infer" | "variable",
  /** Ignore spaces and tabs before an unquoted or quoted field. */
  readonly trimLeadingSpace?: boolean,
};

export type StringifyOptions = {
  /** Field separator. Defaults to `,`. Must be one character. */
  readonly comma?: string,
  /** Record separator. Defaults to `\n`; `\r\n` is the RFC 4180 shape. */
  readonly lineTerminator?: "\n" | "\r\n",
};

/** A malformed CSV document, with a one-based line and column. */
export class InvalidCsvError extends Error {
  line: number;
  column: number;

  constructor(message: string, line: number, column: number) {
    super(`${message} at ${String(line)}:${String(column)}`);
    this.name = "InvalidCsvError";
    this.line = line;
    this.column = column;
  }
}

/** Parse RFC 4180-style CSV text into rows. */
export function parse(
  source: string,
  options?: ParseOptions,
): $ReadOnlyArray<$ReadOnlyArray<string>> {
  const comma = checkedComma(options?.comma ?? ",");
  const trimLeadingSpace = options?.trimLeadingSpace === true;
  const fieldsPerRecord = options?.fieldsPerRecord ?? "infer";
  if (
    typeof fieldsPerRecord === "number" &&
    (!Number.isSafeInteger(fieldsPerRecord) || fieldsPerRecord < 0)
  ) {
    throw new RangeError(
      `@uniflowed/std/csv: fieldsPerRecord must be a non-negative safe integer, got ${String(
        fieldsPerRecord,
      )}`,
    );
  }

  const records = [];
  let expectedFields = typeof fieldsPerRecord === "number" ? fieldsPerRecord : null;
  let index = 0;
  let line = 1;
  let column = 1;

  const fail = (message: string): empty => {
    throw new InvalidCsvError(`@uniflowed/std/csv: ${message}`, line, column);
  };

  const consume = (): string => {
    const character = source[index];
    index += 1;
    column += 1;
    return character;
  };

  const consumeLineBreak = (): string => {
    if (source[index] === "\r" && source[index + 1] === "\n") {
      index += 2;
      line += 1;
      column = 1;
      return "\r\n";
    }
    const character = source[index];
    index += 1;
    line += 1;
    column = 1;
    return character;
  };

  const atLineBreak = (): boolean => source[index] === "\n" || source[index] === "\r";

  const skipLeadingSpace = (): void => {
    if (!trimLeadingSpace) {
      return;
    }
    while (source[index] === " " || source[index] === "\t") {
      consume();
    }
  };

  const readUnquoted = (): string => {
    let field = "";
    while (index < source.length && source[index] !== comma && !atLineBreak()) {
      if (source[index] === '"') {
        fail("bare quote in unquoted field");
      }
      field += consume();
    }
    return field;
  };

  const readQuoted = (): string => {
    const startLine = line;
    const startColumn = column;
    consume();
    let field = "";
    for (;;) {
      if (index >= source.length) {
        throw new InvalidCsvError(
          "@uniflowed/std/csv: unterminated quoted field",
          startLine,
          startColumn,
        );
      }
      if (source[index] === '"') {
        consume();
        if (source[index] === '"') {
          field += consume();
          continue;
        }
        break;
      }
      field += atLineBreak() ? consumeLineBreak() : consume();
    }
    if (index < source.length && source[index] !== comma && !atLineBreak()) {
      fail("extraneous data after quoted field");
    }
    return field;
  };

  const finishRecord = (record: Array<string>): void => {
    if (expectedFields == null && fieldsPerRecord === "infer") {
      expectedFields = record.length;
    }
    if (expectedFields != null && record.length !== expectedFields) {
      fail(`expected ${String(expectedFields)} fields, got ${String(record.length)}`);
    }
    records.push(record);
  };

  while (index < source.length) {
    const record = [];
    for (;;) {
      skipLeadingSpace();
      record.push(source[index] === '"' ? readQuoted() : readUnquoted());

      if (index >= source.length) {
        finishRecord(record);
        break;
      }
      if (source[index] === comma) {
        consume();
        if (index >= source.length) {
          record.push("");
          finishRecord(record);
          break;
        }
        continue;
      }
      if (atLineBreak()) {
        consumeLineBreak();
        finishRecord(record);
        break;
      }
      fail("expected field separator or record terminator");
    }
  }

  return records;
}

/** Serialize rows as CSV text. */
export function stringify(
  records: $ReadOnlyArray<$ReadOnlyArray<string>>,
  options?: StringifyOptions,
): string {
  const comma = checkedComma(options?.comma ?? ",");
  const lineTerminator = options?.lineTerminator ?? "\n";
  return records
    .map((record) =>
      record
        .map((field) => {
          if (mustQuote(field, comma)) {
            return `"${field.replace(/"/g, '""')}"`;
          }
          return field;
        })
        .join(comma),
    )
    .join(lineTerminator);
}

function checkedComma(comma: string): string {
  if (comma.length !== 1 || comma === '"' || comma === "\r" || comma === "\n") {
    throw new RangeError(
      "@uniflowed/std/csv: comma must be one character other than quote or newline",
    );
  }
  return comma;
}

function mustQuote(field: string, comma: string): boolean {
  return (
    field.includes(comma) ||
    field.includes('"') ||
    field.includes("\r") ||
    field.includes("\n") ||
    field.startsWith(" ") ||
    field.endsWith(" ") ||
    field.startsWith("\t") ||
    field.endsWith("\t")
  );
}
