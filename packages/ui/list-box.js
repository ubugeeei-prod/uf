// @flow
"use client";
import * as React from "@uniflowed/react";
import { CollectionRoot } from "./internal/collection.js";
import type { CollectionProps } from "./internal/collection.js";
export component ListBox(...props: CollectionProps) {
  return <CollectionRoot options={props} kind="listbox" />;
}
export type {
  CollectionItem,
  CollectionItemState,
  CollectionProps,
} from "./internal/collection.js";
