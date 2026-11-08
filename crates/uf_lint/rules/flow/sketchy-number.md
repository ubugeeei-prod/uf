A number on the left of `&&` in JSX renders `0` when it is zero, instead of rendering nothing. Turn it into a boolean first.

## Bad

```js
"use flow";
export component Unread(count: number) {
  return <p>{count && <span>{count} new</span>}</p>;
}
```

## Good

```js
"use flow";
export component Unread(count: number) {
  return <p>{count > 0 && <span>{count} new</span>}</p>;
}
```
