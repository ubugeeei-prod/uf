`scope` tells a screen reader which cells a header labels. It means that only on `th`; anywhere else it is ignored (WCAG 1.3.1).

## Bad

```js
// @flow
export component Prices() {
  return (
    <table>
      <tbody>
        <tr>
          <td scope="row">Basic</td>
          <td>$5</td>
        </tr>
      </tbody>
    </table>
  );
}
```

```diagnostics
app/example.js:7:15 HTML defines `scope` on `<th>` and nowhere else, so on a `<td>` it is ignored and the header it was meant to describe is not announced as a header at all; make this a `<th scope="col">` or `<th scope="row">`, which is what lets a screen reader read a data table cell by cell
```

## Good

```js
// @flow
export component Prices() {
  return (
    <table>
      <tbody>
        <tr>
          <th scope="row">Basic</th>
          <td>$5</td>
        </tr>
      </tbody>
    </table>
  );
}
```
