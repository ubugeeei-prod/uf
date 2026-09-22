// @flow
"use client";
import * as React from "@uniflowed/react";
import { CollectionRoot } from "./internal/collection.js";
import type { CollectionProps } from "./internal/collection.js";
export component TagGroup(...props: CollectionProps) {
  return <CollectionRoot options={props} kind="tags" />;
}
