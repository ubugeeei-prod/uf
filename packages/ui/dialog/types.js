// @flow
import type { HeadlessComponent } from "../types/renders.js";

export type DialogParts = {
  readonly Root: HeadlessComponent,
  readonly Trigger: HeadlessComponent,
  readonly Overlay: HeadlessComponent,
  readonly Body: HeadlessComponent,
  readonly Header: HeadlessComponent,
  readonly Footer: HeadlessComponent,
  readonly Title: HeadlessComponent,
  readonly Description: HeadlessComponent,
  readonly Close: HeadlessComponent,
};
