// @flow
"use client";
import * as React from "@uniflowed/react";
import { SegmentedField } from "./internal/segmented-field.js";
import type { DateFieldProps } from "./internal/segmented-field.js";
export component DateField(...props: DateFieldProps) {
  return <SegmentedField options={props} time={false} />;
}
export type { DateFieldProps } from "./internal/segmented-field.js";
