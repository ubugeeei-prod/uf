An `invariant` whose condition Flow already knows is true checks nothing, and suggests to a reader that the value could be missing when it cannot.

## Bad

```js
// @flow
import invariant from "invariant";

export function first(items: [string, string]): string {
  const head = items[0];
  invariant(head, "a pair has a first item");
  return head;
}
```

## Good

```js
// @flow
import invariant from "invariant";

export function first(items: $ReadOnlyArray<string>): string {
  const head = items[0];
  invariant(head != null, "the list is not empty");
  return head;
}
```
