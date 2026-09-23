A `uf-lint-disable` comment that names a rule uf does not know suppresses nothing, but it looks like it works. A typo such as `flow/unclear-typo` leaves the finding reported, or worse, leaves a later reader believing the line was reviewed. uf reports the unknown name, so a suppression either works or fails loudly.

## Bad

```js
// @flow
// uf-lint-disable-next-line flow/unclear-typo
export type Payload = { data: mixed };
```

```diagnostics
app/example.js:2:30 unknown lint rule `flow/unclear-typo` in suppression comment
```

## Good

```js
// @flow
// The payload is validated by the caller, and Flow cannot express its shape.
// uf-lint-disable-next-line flow/unclear-type
export type Payload = { data: any };
```
