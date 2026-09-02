// @flow
//
// `@uniflowed/ui`.
//
// Every component is a placeholder that raises when it is rendered and names
// the part it stands for, so a missing runtime reports `Dialog.Trigger` rather
// than a bare `undefined is not a function`. Each top-level binding carries a
// pure annotation: without it a bundler must assume a top-level call could have
// side effects and would keep all forty-eight of them.

import type * as React from "./react.js";
import type { Schema } from "./validator.js";
import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/ui";

export type HeadlessProps = {
  +className?: string,
  +children?: React.Node,
  +variant?: string,
  +size?: string,
  +disabled?: boolean,
};

export type HeadlessComponent = component(
  className?: string,
  children?: React.Node,
  variant?: string,
  size?: string,
  disabled?: boolean,
) renders React.Node;

export type CompoundComponent = {
  +Root: HeadlessComponent,
  +Trigger?: HeadlessComponent,
  +Overlay?: HeadlessComponent,
  +Body?: HeadlessComponent,
  +Header?: HeadlessComponent,
  +Footer?: HeadlessComponent,
  +Title?: HeadlessComponent,
  +Description?: HeadlessComponent,
  +Close?: HeadlessComponent,
  +Item?: HeadlessComponent,
  +Content?: HeadlessComponent,
  +List?: HeadlessComponent,
  +Input?: HeadlessComponent,
  +Label?: HeadlessComponent,
  +Control?: HeadlessComponent,
  +Message?: HeadlessComponent,
};

export type FormRoot = component<T: {...}>(
  schema: Schema<T>,
  defaultValue?: T,
  children?: React.Node,
  onSubmit?: (value: T) => mixed | Promise<mixed>,
) renders React.Node;

export type FormComponent = {
  +Root: FormRoot,
  +Field: HeadlessComponent,
  +Label: HeadlessComponent,
  +Control: HeadlessComponent,
  +Message: HeadlessComponent,
  +Submit: HeadlessComponent,
};

/** Placeholder for one headless part, e.g. `Dialog.Trigger`. */
function headless(binding: string): HeadlessComponent {
  return function HeadlessBinding(props: HeadlessProps): empty {
    return nativeRuntimeRequired(MODULE, binding);
  };
}

/**
 * Placeholder for a compound component: every optional part is materialised so
 * `Dialog.Body` is a component that raises rather than `undefined`.
 */
function compound(name: string): CompoundComponent {
  return {
    Root: headless(`${name}.Root`),
    Trigger: headless(`${name}.Trigger`),
    Overlay: headless(`${name}.Overlay`),
    Body: headless(`${name}.Body`),
    Header: headless(`${name}.Header`),
    Footer: headless(`${name}.Footer`),
    Title: headless(`${name}.Title`),
    Description: headless(`${name}.Description`),
    Close: headless(`${name}.Close`),
    Item: headless(`${name}.Item`),
    Content: headless(`${name}.Content`),
    List: headless(`${name}.List`),
    Input: headless(`${name}.Input`),
    Label: headless(`${name}.Label`),
    Control: headless(`${name}.Control`),
    Message: headless(`${name}.Message`),
  };
}

/** Placeholder for the schema-bound `Form.Root`. */
function formRoot(): FormRoot {
  return function FormRootBinding<T: {...}>(props: {
    +schema: Schema<T>,
    +defaultValue?: T,
    +children?: React.Node,
    +onSubmit?: (value: T) => mixed | Promise<mixed>,
  }): empty {
    return nativeRuntimeRequired(MODULE, "Form.Root");
  };
}

export const Accordion: CompoundComponent =
  /*#__PURE__*/ compound("Accordion");
export const Alert: CompoundComponent = /*#__PURE__*/ compound("Alert");
export const AlertDialog: CompoundComponent =
  /*#__PURE__*/ compound("AlertDialog");
export const AspectRatio: CompoundComponent =
  /*#__PURE__*/ compound("AspectRatio");
export const Avatar: CompoundComponent = /*#__PURE__*/ compound("Avatar");
export const Badge: HeadlessComponent = /*#__PURE__*/ headless("Badge");
export const Breadcrumb: CompoundComponent =
  /*#__PURE__*/ compound("Breadcrumb");
export const Button: HeadlessComponent = /*#__PURE__*/ headless("Button");
export const Calendar: CompoundComponent = /*#__PURE__*/ compound("Calendar");
export const Card: CompoundComponent = /*#__PURE__*/ compound("Card");
export const Carousel: CompoundComponent = /*#__PURE__*/ compound("Carousel");
export const Chart: CompoundComponent = /*#__PURE__*/ compound("Chart");
export const Checkbox: CompoundComponent = /*#__PURE__*/ compound("Checkbox");
export const Collapsible: CompoundComponent =
  /*#__PURE__*/ compound("Collapsible");
export const Command: CompoundComponent = /*#__PURE__*/ compound("Command");
export const ContextMenu: CompoundComponent =
  /*#__PURE__*/ compound("ContextMenu");
export const DataTable: CompoundComponent =
  /*#__PURE__*/ compound("DataTable");
export const DatePicker: CompoundComponent =
  /*#__PURE__*/ compound("DatePicker");
export const Dialog: CompoundComponent = /*#__PURE__*/ compound("Dialog");
export const Drawer: CompoundComponent = /*#__PURE__*/ compound("Drawer");
export const DropdownMenu: CompoundComponent =
  /*#__PURE__*/ compound("DropdownMenu");
export const Form: FormComponent = {
  Root: /*#__PURE__*/ formRoot(),
  Field: /*#__PURE__*/ headless("Form.Field"),
  Label: /*#__PURE__*/ headless("Form.Label"),
  Control: /*#__PURE__*/ headless("Form.Control"),
  Message: /*#__PURE__*/ headless("Form.Message"),
  Submit: /*#__PURE__*/ headless("Form.Submit"),
};
export const HoverCard: CompoundComponent =
  /*#__PURE__*/ compound("HoverCard");
export const Input: HeadlessComponent = /*#__PURE__*/ headless("Input");
export const InputOtp: CompoundComponent = /*#__PURE__*/ compound("InputOtp");
export const Label: HeadlessComponent = /*#__PURE__*/ headless("Label");
export const Menubar: CompoundComponent = /*#__PURE__*/ compound("Menubar");
export const NavigationMenu: CompoundComponent =
  /*#__PURE__*/ compound("NavigationMenu");
export const Pagination: CompoundComponent =
  /*#__PURE__*/ compound("Pagination");
export const Popover: CompoundComponent = /*#__PURE__*/ compound("Popover");
export const Progress: HeadlessComponent = /*#__PURE__*/ headless("Progress");
export const RadioGroup: CompoundComponent =
  /*#__PURE__*/ compound("RadioGroup");
export const Resizable: CompoundComponent =
  /*#__PURE__*/ compound("Resizable");
export const ScrollArea: CompoundComponent =
  /*#__PURE__*/ compound("ScrollArea");
export const Select: CompoundComponent = /*#__PURE__*/ compound("Select");
export const Separator: HeadlessComponent =
  /*#__PURE__*/ headless("Separator");
export const Sheet: CompoundComponent = /*#__PURE__*/ compound("Sheet");
export const Sidebar: CompoundComponent = /*#__PURE__*/ compound("Sidebar");
export const Skeleton: HeadlessComponent = /*#__PURE__*/ headless("Skeleton");
export const Slider: CompoundComponent = /*#__PURE__*/ compound("Slider");
export const Sonner: CompoundComponent = /*#__PURE__*/ compound("Sonner");
export const Switch: CompoundComponent = /*#__PURE__*/ compound("Switch");
export const Table: CompoundComponent = /*#__PURE__*/ compound("Table");
export const Tabs: CompoundComponent = /*#__PURE__*/ compound("Tabs");
export const Textarea: HeadlessComponent = /*#__PURE__*/ headless("Textarea");
export const Toast: CompoundComponent = /*#__PURE__*/ compound("Toast");
export const Toggle: HeadlessComponent = /*#__PURE__*/ headless("Toggle");
export const ToggleGroup: CompoundComponent =
  /*#__PURE__*/ compound("ToggleGroup");
export const Tooltip: CompoundComponent = /*#__PURE__*/ compound("Tooltip");
