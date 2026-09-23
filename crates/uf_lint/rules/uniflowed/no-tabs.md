A tab is as wide as each editor, terminal and code review tool decides, so tab-indented code lines up differently for every reader. uf formats with spaces, and `uf fmt` fixes this. A tab inside a string is part of the string and is left alone.

## Bad

```js
// @flow
export function total(prices: $ReadOnlyArray<number>): number {
	return prices.reduce((sum, price) => sum + price, 0);
}
```

```diagnostics
app/example.js:3:1 replace tabs with spaces
```

## Good

```js
// @flow
export function total(prices: $ReadOnlyArray<number>): number {
  return prices.reduce((sum, price) => sum + price, 0);
}

export const columns = "name\tprice";
```
