A `Promise` that nobody awaits or handles fails silently: its rejection becomes an unhandled rejection, and whatever comes next runs before it has settled. Await it, return it, or attach a handler.

## Bad

```js
// @flow
declare function save(value: string): Promise<void>;

export async function submit(value: string): Promise<void> {
  save(value);
}
```

## Good

```js
// @flow
declare function save(value: string): Promise<void>;

export async function submit(value: string): Promise<void> {
  await save(value);
}
```
