// @flow
import type * as React from "@uniflowed/react";
import type { Schema } from "@uniflowed/validator";
import type { HeadlessComponent } from "../types/renders.js";

export type FormState<T: {...}> = {
  +value: T,
  +errors: { +[string]: $ReadOnlyArray<string> },
  +valid: boolean,
};

export type FormRootProps<T: {...}> = {
  +schema: Schema<T>,
  +defaultValue?: T,
  +children?: React.Node,
  +onSubmit?: (value: T) => mixed | Promise<mixed>,
};

export type FormRoot = component<T: {...}>(
  schema: Schema<T>,
  defaultValue?: T,
  children?: React.Node,
  onSubmit?: (value: T) => mixed | Promise<mixed>,
) renders React.Node;

export type FormParts = {
  +Root: FormRoot,
  +Field: HeadlessComponent,
  +Label: HeadlessComponent,
  +Control: HeadlessComponent,
  +Message: HeadlessComponent,
  +Submit: HeadlessComponent,
};
