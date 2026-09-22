// @flow
import * as React from "@uniflowed/react";
import { TagGroup } from "./tag-group.js";
const items = [
  { key: "alpha", textValue: "Alpha" },
  { key: "beta", textValue: "Beta" },
  { key: "gamma", textValue: "Gamma", disabled: true },
];

/** A named, keyboard-operable example using the default presentation. */
export component Example() {
  const [tags, setTags] = React.useState(items);
  return (
    <TagGroup
      aria-label="Tags"
      items={tags}
      onRemove={(key) => setTags(tags.filter((item) => item.key !== key))}
    />
  );
}
