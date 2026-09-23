Deaf and hard-of-hearing viewers rely on captions for anything a video or recording says (WCAG 1.2.2). Give `audio` and `video` a `track` of kind `captions`.

## Bad

```js
// @flow
export component Intro() {
  return (
    <video src="/intro.mp4" controls />
  );
}
```

```diagnostics
app/example.js:4:5 this `<video>` has no `<track kind="captions">`, so whoever cannot hear it misses what is said (WCAG 1.2.2); add a captions track, or `muted` if it has no sound to caption
```

## Good

```js
// @flow
export component Intro() {
  return (
    <video src="/intro.mp4" controls>
      <track kind="captions" src="/intro.en.vtt" srcLang="en" />
    </video>
  );
}
```
