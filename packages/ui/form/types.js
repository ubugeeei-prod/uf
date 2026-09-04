// @flow
import type * as React from "@uniflowed/react";
import type { Schema } from "@uniflowed/validator";
import type { HeadlessComponent } from "../types/renders.js";

export type FormState<T extends { ... }> = {
  readonly value: T,
  readonly errors: { readonly [string]: $ReadOnlyArray<string> },
  readonly valid: boolean,
};

export type FormRootProps<T extends { ... }> = {
  readonly schema: Schema<T>,
  readonly defaultValue?: T,
  readonly children?: React.Node,
  readonly onSubmit?: (value: T) => mixed | Promise<mixed>,
};

export type FormRoot = component<T extends { ... }>(
  schema: Schema<T>,
  defaultValue?: T,
  children?: React.Node,
  onSubmit?: (value: T) => mixed | Promise<mixed>,
) renders React.Node;

export type FormParts = {
  readonly Root: FormRoot,
  readonly Field: HeadlessComponent,
  readonly Label: HeadlessComponent,
  readonly Control: HeadlessComponent,
  readonly Message: HeadlessComponent,
  readonly Submit: HeadlessComponent,
};
