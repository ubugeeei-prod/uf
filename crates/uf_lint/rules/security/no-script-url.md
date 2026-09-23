A `javascript:` URL in `src`, `action`, `formAction` or an `area`'s `href` is a program the browser runs when the URL is loaded or followed. React 19 blocks it and warns, and before that it was a cross-site scripting hole. Use an event handler for an action and a real URL for a resource. A `javascript:` URL in an `a` element's `href` is reported by `a11y/anchor-is-valid` instead, so only one rule speaks about it.

## Bad

```js
// @flow
export component Search() {
  return (
    <form action="javascript:search()">
      <input aria-label="Query" name="q" />
    </form>
  );
}
```

```diagnostics
app/example.js:4:11 `action` on this `<form>` is a `javascript:` URL, which is not a place to go but a program to run, with everything the page can do; browsers and React both block it in more and more positions, and the moment any part of the string comes from outside the module it is an injection rather than a quirk — attach an `onClick` handler and give the element a real destination, or none at all
```

## Good

```js
// @flow
declare function search(data: FormData): void;

export component Search() {
  return (
    <form action={search}>
      <input aria-label="Query" name="q" />
    </form>
  );
}
```
