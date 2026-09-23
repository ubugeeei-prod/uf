An `a` without a real `href` is not a link. It is not in the tab order, it opens nothing in a new tab, and it is announced as something it is not. Something that performs an action is a `button`.

## Bad

```js
// @flow
export component Menu(onOpen: () => void) {
  return (
    <a href="#" onClick={onOpen}>
      Open menu
    </a>
  );
}
```

```diagnostics
app/example.js:4:8 `href="#"` jumps to the top of the page, so with an `onClick` this `<a>` is a button that a screen reader calls a link, and Space, the key that presses a button, scrolls the page instead; make it a `<button type="button">`, or give it a real destination
```

## Good

```js
// @flow
export component Menu(onOpen: () => void) {
  return (
    <button type="button" onClick={onOpen}>
      Open menu
    </button>
  );
}
```
