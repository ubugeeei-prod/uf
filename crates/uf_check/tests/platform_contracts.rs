//! Platform contracts used by the repository, with valid and invalid uses.

#![cfg(feature = "upstream-typecheck")]

use uf_check::{CheckLimits, Source, check_source};

fn contract(source: &str) {
    let expected: Vec<_> = source
        .lines()
        .enumerate()
        .filter_map(|(index, line)| line.contains("// error").then_some(index as u32 + 2))
        .collect();
    let report = check_source(
        Source::new("platform_contract.js", source),
        &[],
        &CheckLimits::default().without_timeout(),
    )
    .expect("the checker runs");
    let mut actual: Vec<_> = report
        .iter()
        .map(|diagnostic| diagnostic.primary.start.line)
        .collect();
    actual.sort_unstable();
    actual.dedup();
    assert_eq!(actual, expected, "{report:#?}");
}

#[test]
fn fetch_accepts_readonly_inputs_and_exposes_bodies_and_cookies() {
    contract(
        r#"// @flow
const record: { readonly [string]: string } = { accept: "text/html" };
const headers = new Headers(record);
const cookies: Array<string> = headers.getSetCookie();
const params = new URLSearchParams(record);
const pairs: $ReadOnlyArray<[string, string]> = [["x", "y"]];
new Headers(pairs);
new URLSearchParams(pairs);
const request = new Request("http://uf.test");
const body: ?ReadableStream<Uint8Array> = request.body;
const keepalive: boolean = request.keepalive;
// error
const wrongCookies: number = headers.getSetCookie();
// error
new Headers({ bad: 42 });
// error
const wrongBody: string = request.body;
"#,
    );
}

#[test]
fn async_local_storage_preserves_context_and_callback_types() {
    contract(
        r#"// @flow
import { AsyncLocalStorage } from "node:async_hooks";
import { AsyncLocalStorage as Legacy } from "async_hooks";
const storage: AsyncLocalStorage<string> = new AsyncLocalStorage();
const store: string | void = storage.getStore();
const result: number = storage.run("request", (value: number) => value + 1, 2);
const exited: string = storage.exit(() => "outside");
storage.enterWith("next");
const bound = AsyncLocalStorage.bind((n: number): string => String(n));
const answer: string = bound(1);
const captured = AsyncLocalStorage.snapshot();
const snap: number = captured((n: number) => n + 1, 2);
const legacy: Legacy<number> = new Legacy();
legacy.enterWith(1);
storage.disable();
// error
storage.enterWith(42);
// error
const wrong: string = result;
// error
bound("no");
"#,
    );
}

#[test]
fn directory_entries_follow_the_requested_encoding() {
    contract(
        r#"// @flow
import fs from "node:fs";
const entries = fs.readdirSync(".", { withFileTypes: true });
const name: string = entries[0].name;
entries[0].name.endsWith(".js");
const encoded = fs.readdirSync(".", { withFileTypes: true, encoding: "buffer" });
const bytes: Buffer = encoded[0].name;
const raw: Array<Buffer> = fs.readdirSync(".", "buffer");
const names: Array<string> = fs.readdirSync(".");
fs.readdir(".", { withFileTypes: true }, (_error, files) => {
  const path: string = files[0].name;
});
fs.readdir(".", { withFileTypes: true, encoding: "buffer" }, (_error, files) => {
  const path: Buffer = files[0].name;
});
// error
const wrongString: string = encoded[0].name;
// error
const wrongBuffer: Buffer = entries[0].name;
"#,
    );
}

#[test]
fn function_constructor_accepts_strings_and_rejects_other_arguments() {
    contract(
        r#"// @flow
const empty = new Function();
const body = new Function("return 1;");
const argumentsAndBody = new Function("a", "b", "return a + b;");
const source: string = argumentsAndBody.toString();
// error
new Function(42);
"#,
    );
}

#[test]
fn parcel_flight_entry_points_keep_transport_and_reference_types() {
    contract(
        r#"// @flow
import { createFromFetch, setServerCallback } from "react-server-dom-parcel/client.browser";
import { createFromReadableStream } from "react-server-dom-parcel/client.edge";
import { renderToReadableStream, registerServerReference } from "react-server-dom-parcel/server";
const stream: ReadableStream<Uint8Array> = renderToReadableStream({ tree: "hello" });
const model: Promise<{ tree: string }> = createFromReadableStream(stream);
const responseModel: Promise<string> = createFromFetch(Promise.resolve(new Response("flight")));
setServerCallback(async (id: string, args: Array<mixed>): Promise<mixed> => id);
const action = registerServerReference(async (n: number): Promise<string> => String(n), "id", "name");
const answer: Promise<string> = action(1);
// error
createFromReadableStream(42);
// error
action("no");
// error
import { setServerCallback as unavailable } from "react-server-dom-parcel/client.edge";
// error
import { nonexistent } from "react-server-dom-parcel/server";
"#,
    );
}
