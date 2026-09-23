uf's tracing, caching and request mocking are all built around the global `fetch`. Replacing it, as in `globalThis.fetch = …`, silently unhooks them for the whole process, including other packages. Wrap `fetch` in a function of your own, or use `@uniflowed/mock` in tests.

## Bad

```js path=src/client.js
// @flow
const original = globalThis.fetch;
globalThis.fetch = (input, init) => original(input, { ...init, credentials: "include" });
```

```diagnostics
src/client.js:3:1 do not override global fetch; use @uniflowed/fetch explicit clients
```

## Good

```js path=src/client.js
// @flow
export function request(input: string, init?: RequestOptions): Promise<Response> {
  return fetch(input, { ...init, credentials: "include" });
}
```
