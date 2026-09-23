`any`, `Object` and `Function` switch Flow off for whatever they touch: a value typed `any` accepts every operation, and so does everything it flows into. Say what the value is instead: `mixed` when it could be anything, an object or function type when you know its shape.

## Bad

```js
// @flow
export type Props = { value: any };
```

```diagnostics
app/example.js:2:30 avoid `any`; use `mixed`, opaque types, or generated router/action types
```

## Good

```js
// @flow
export type Props = { value: mixed };
```
