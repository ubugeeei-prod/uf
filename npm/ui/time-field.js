// @flow
"use client";
import * as React from "@uniflowed/react";
import { SegmentedField } from "./internal/segmented-field.js";
import type { DateFieldProps } from "./internal/segmented-field.js";
export component TimeField(...props: DateFieldProps) {
  return <SegmentedField options={props} time={true} />;
}
