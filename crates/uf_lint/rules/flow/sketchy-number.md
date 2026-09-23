A number on the left of `&&` in JSX renders `0` when it is zero, instead of rendering nothing. Turn it into a boolean first.

## Bad

```js
// @flow
export component Unread(count: number) {
  return <p>{count && <span>{count} new</span>}</p>;
}
```

## Good

```js
// @flow
export component Unread(count: number) {
  return <p>{count > 0 && <span>{count} new</span>}</p>;
}
```
