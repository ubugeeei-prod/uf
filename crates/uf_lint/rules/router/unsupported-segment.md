An intercepting segment such as `(.)photo` renders a route in place of another, and uf supports it only inside a `@slot`, where there is somewhere to render it. Outside a slot, the directory is refused, where before it was silently treated as a literal URL segment. Move the interception under a slot such as `@modal`.

## Bad

```js path=app/feed/(.)photo/$page.js
// @flow
export component Page() {
  return <img alt="" src="/photo.jpg" />;
}
```

```diagnostics
app/feed/(.)photo/$page.js:1:1 `(.)photo` is an intercepting route, and an intercepting route renders into a `@slot`: it is what a client navigation shows in a named place instead of the page its URL names, and outside a slot there is no named place for it to show in. It is refused rather than served as the URL segment `/(.)photo`, which is what it used to become. Move it inside a slot directory beside the layout that renders the slot, or rename the directory to the literal segment `photo`. https://github.com/ubugeeei-prod/uf/issues/267
```

## Good

```js path=app/feed/@modal/(.)photo/$page.js
// @flow
export component Page() {
  return <img alt="" src="/photo.jpg" />;
}
```
