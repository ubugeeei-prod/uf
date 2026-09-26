// @flow
// Correct inferred public types and the invalid assignments they must reject.

import { useInfiniteQuery, useQuery } from "../../packages/query/index.js";
import { ufTokens } from "../../packages/stylex/tokens.stylex.js";
import type { ThemeOverrides } from "../../packages/stylex/index.js";
import { BufferWriter, BytesReader, copy, readAll } from "../../packages/std/io.js";
import type { Reader, Writer } from "../../packages/std/io.js";
import { BufferedReader } from "../../packages/std/bufio.js";

export hook useInferredQueryContracts(): void {
  const plain = useQuery({ queryKey: ["plain"], queryFn: async () => ["ada"] });
  const names: string = plain.data?.join(",") ?? "";
  // $FlowExpectedError[incompatible-type] queryFn data stays an array
  const wrongPlain: number | void = plain.data;

  const selected = useQuery({
    queryKey: ["selected"],
    queryFn: async () => ["ada"],
    select: (value: Array<string>) => value.length,
  });
  const count: number | void = selected.data;
  // $FlowExpectedError[incompatible-type] select decides the result type
  const wrongSelected: string | void = selected.data;

  const paged = useInfiniteQuery({
    queryKey: ["paged"],
    queryFn: async ({ pageParam }) => ({ items: ["ada"], page: pageParam }),
    initialPageParam: 0,
    getNextPageParam: () => null,
  });
  const pageItems: Array<string> | void = paged.data?.pages[0].items;
  // $FlowExpectedError[incompatible-type] an unselected page keeps its item type
  const wrongPage: number | void = paged.data?.pages[0].items;
}

const theme: ThemeOverrides<typeof ufTokens> = { accent: "#123456" };
// $FlowExpectedError[incompatible-type] only shipped token keys can be overridden
const wrongTheme: ThemeOverrides<typeof ufTokens> = { invented: "#123456" };

const reader: Reader = { read: async () => ({ done: true }) };
const writer: Writer = { write: (chunk: Uint8Array) => chunk.length };
const bytes = new BytesReader(new Uint8Array([1, 2]));
const collected = new BufferWriter();
const buffered = new BufferedReader(bytes);
const copied: Promise<number> = copy(collected, reader);
const all: Promise<Uint8Array> = readAll(buffered);
const structurallyCopied: Promise<number> = copy(writer, bytes);
// $FlowExpectedError[incompatible-type] byte writers do not accept text chunks
const wrongWriter: Writer = { write: (chunk: string) => chunk.length };
