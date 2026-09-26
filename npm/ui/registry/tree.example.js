// @flow
import * as React from "@uniflowed/react";
import { Tree } from "./tree.js";

/** A named, keyboard-operable example using the default presentation. */
export component Example() {
  return (
    <Tree
      aria-label="Files"
      items={[
        { key: "folder", textValue: "Folder", children: [{ key: "child", textValue: "Child" }] },
      ]}
    />
  );
}
