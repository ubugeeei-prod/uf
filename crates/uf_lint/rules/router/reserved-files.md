In the app directory, a file name starting with `$` is reserved for the router: `$page`, `$layout`, `$middleware`, `$route`, `$not-found`, `$error` and the rest. A `$` name the router does not define, such as `$handler.js`, is not served as anything, and nothing says so at run time. Use one of the router's names, or a name without `$` for your own module.

## Bad

```js path=app/$handler.js
// @flow
export default function handler(): string {
  return "hello";
}
```

```diagnostics
app/$handler.js:1:1 reserved file names are $<layout|template|page|default|middleware|not-found|error|loading|route|story|instrumentation>[.<native|ios|android|web|test|client>].js
```

## Good

```js path=app/api/$route.js
// @flow
export function GET(): Response {
  return new Response("hello");
}
```
