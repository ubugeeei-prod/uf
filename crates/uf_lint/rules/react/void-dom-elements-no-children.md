`img`, `br`, `input` and the other void elements cannot have children in HTML. React throws when it is given some.

## Bad

```js
// @flow
export component Photo(src: string) {
  return <img src={src}>A cat asleep</img>;
}
```

```diagnostics
app/example.js:3:10 `<img>` is a void element and cannot hold children: React throws when it renders one that does; put the content next to it instead
```

## Good

```js
// @flow
export component Photo(src: string) {
  return <img src={src} alt="A cat asleep" />;
}
```
