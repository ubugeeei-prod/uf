A `uf-lint-disable` comment whose rule finds nothing on the lines it covers suppresses nothing. It is usually left over from a finding that went away: the code changed, or the rule stopped reporting a false positive. Left in place, it would silence the next real finding on that line without anyone reading it. Delete it. Rules that are off, rules that need type inference, and (in an editor linting one file) the project-wide `import/*` rules are not judged, since they did not look.

## Bad

```js
// @flow
// uf-lint-disable-next-line flow/unclear-type
export type Payload = { data: mixed };
```

```diagnostics
app/example.js:2:30 `flow/unclear-type` reports nothing here, so this suppression silences nothing; remove it
```

## Good

```js
// @flow
export type Payload = { data: mixed };
```
