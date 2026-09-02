// @flow
//
// `@uniflowed/tui`.

import type * as React from "@uniflowed/react";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/tui";

export type TuiEngine = "uf-native-open-tui-compatible";
export type TuiStandard = "open-tui";
export type TuiRenderer = "cell-diff-native";
export type TuiLayoutEngine = "flexbox-yoga-compatible";
export type TuiInputModel = "keyboard-mouse-focus-selection";
export type TuiRuntimeBinding = "flow-react";
export type TuiPerformanceTarget = "faster-than-react-ink";

export type TuiFeature =
  | "flexbox"
  | "cell-diff"
  | "keyboard"
  | "mouse"
  | "focus"
  | "selection"
  | "scrollback"
  | "keymap"
  | "in-memory-testing"
  | "snapshot-testing"
  | "terminal-automation"
  | "rich-text"
  | "code-highlight"
  | "markdown"
  | "images"
  | "audio"
  | "three-d"
  | "ssh"
  | "qr-code"
  | "embedded-terminal"
  | "clipboard"
  | "notifications"
  | "animations";

export type TuiComponentKind =
  | "display"
  | "input"
  | "selection"
  | "scrolling"
  | "rich-content"
  | "graphics"
  | "application"
  | "testing"
  | "integration";

export type TuiComponent = {
  +name: string,
  +parts: $ReadOnlyArray<string>,
  +kind: TuiComponentKind,
  +serverComponentSafe: boolean,
  +interactive: boolean,
  +feature: TuiFeature,
};

export type ReactInkTarget = {
  +replacementReady: true,
  +nativeRenderer: true,
  +typedComponents: true,
  +richMedia: true,
  +inMemoryTests: true,
  +performanceTarget: TuiPerformanceTarget,
};

export type TuiFrameworkContract = {
  +engine: TuiEngine,
  +standard: TuiStandard,
  +renderer: TuiRenderer,
  +layout: TuiLayoutEngine,
  +input: TuiInputModel,
  +runtimeBinding: TuiRuntimeBinding,
  +features: $ReadOnlyArray<TuiFeature>,
  +components: $ReadOnlyArray<TuiComponent>,
  +reactInkTarget: ReactInkTarget,
};

export type TuiProps = {
  +children?: React.Node,
  +id?: string,
  +width?: number | string,
  +height?: number | string,
  +grow?: number,
  +shrink?: number,
  +focusable?: boolean,
};

export type TuiComponentFn = component(
  children?: React.Node,
  id?: string,
  width?: number | string,
  height?: number | string,
  grow?: number,
  shrink?: number,
  focusable?: boolean,
) renders React.Node;

export type SelectComponent = {
  +Root: TuiComponentFn,
  +Item: TuiComponentFn,
  +Group: TuiComponentFn,
  +Empty: TuiComponentFn,
};

export type ScrollBoxComponent = {
  +Root: TuiComponentFn,
  +Viewport: TuiComponentFn,
  +Content: TuiComponentFn,
};

export type FrameBufferComponent = {
  +Root: TuiComponentFn,
  +Layer: TuiComponentFn,
};

export type RenderTuiHandle = {
  +stop: () => void,
  +snapshot: () => string,
};

export type RenderTuiOptions = {
  +stdin?: mixed,
  +stdout?: mixed,
  +testing?: boolean,
};

/**
 * Build the placeholder for one native terminal component.
 *
 * Rendering it raises and names the binding; importing it does not, so a
 * bundler is free to drop the components an application never mounts. Every
 * call site carries a pure annotation for exactly that reason: without it a
 * bundler must assume a top-level call could have side effects and keeps all
 * of them.
 */
function tuiComponent(binding: string): TuiComponentFn {
  return function TuiBinding(props: TuiProps): empty {
    return nativeRuntimeRequired(MODULE, binding);
  };
}

export function contract(): TuiFrameworkContract {
  return nativeRuntimeRequired(MODULE, "contract");
}

export function renderTui(
  node: React.Node,
  options?: RenderTuiOptions,
): RenderTuiHandle {
  return nativeRuntimeRequired(MODULE, "renderTui");
}

export const Box: TuiComponentFn = /*#__PURE__*/ tuiComponent("Box");
export const Text: TuiComponentFn = /*#__PURE__*/ tuiComponent("Text");
export const Input: TuiComponentFn = /*#__PURE__*/ tuiComponent("Input");
export const Textarea: TuiComponentFn = /*#__PURE__*/ tuiComponent("Textarea");
export const Select: SelectComponent = {
  Root: /*#__PURE__*/ tuiComponent("Select.Root"),
  Item: /*#__PURE__*/ tuiComponent("Select.Item"),
  Group: /*#__PURE__*/ tuiComponent("Select.Group"),
  Empty: /*#__PURE__*/ tuiComponent("Select.Empty"),
};
export const TabSelect: TuiComponentFn =
  /*#__PURE__*/ tuiComponent("TabSelect");
export const Slider: TuiComponentFn = /*#__PURE__*/ tuiComponent("Slider");
export const ScrollBox: ScrollBoxComponent = {
  Root: /*#__PURE__*/ tuiComponent("ScrollBox.Root"),
  Viewport: /*#__PURE__*/ tuiComponent("ScrollBox.Viewport"),
  Content: /*#__PURE__*/ tuiComponent("ScrollBox.Content"),
};
export const ScrollBar: TuiComponentFn =
  /*#__PURE__*/ tuiComponent("ScrollBar");
export const Code: TuiComponentFn = /*#__PURE__*/ tuiComponent("Code");
export const Markdown: TuiComponentFn = /*#__PURE__*/ tuiComponent("Markdown");
export const LineNumbers: TuiComponentFn =
  /*#__PURE__*/ tuiComponent("LineNumbers");
export const Diff: TuiComponentFn = /*#__PURE__*/ tuiComponent("Diff");
export const TextTable: TuiComponentFn =
  /*#__PURE__*/ tuiComponent("TextTable");
export const AsciiFont: TuiComponentFn =
  /*#__PURE__*/ tuiComponent("AsciiFont");
export const FrameBuffer: FrameBufferComponent = {
  Root: /*#__PURE__*/ tuiComponent("FrameBuffer.Root"),
  Layer: /*#__PURE__*/ tuiComponent("FrameBuffer.Layer"),
};
export const Image: TuiComponentFn = /*#__PURE__*/ tuiComponent("Image");
export const QrCode: TuiComponentFn = /*#__PURE__*/ tuiComponent("QrCode");
export const EmbeddedTerminal: TuiComponentFn =
  /*#__PURE__*/ tuiComponent("EmbeddedTerminal");
export const Clipboard: TuiComponentFn =
  /*#__PURE__*/ tuiComponent("Clipboard");
export const Notification: TuiComponentFn =
  /*#__PURE__*/ tuiComponent("Notification");
export const Audio: TuiComponentFn = /*#__PURE__*/ tuiComponent("Audio");
export const Timeline: TuiComponentFn = /*#__PURE__*/ tuiComponent("Timeline");
export const Keymap: TuiComponentFn = /*#__PURE__*/ tuiComponent("Keymap");
export const SshHost: TuiComponentFn = /*#__PURE__*/ tuiComponent("SshHost");
export const ThreeCanvas: TuiComponentFn =
  /*#__PURE__*/ tuiComponent("ThreeCanvas");
export const TestRenderer: TuiComponentFn =
  /*#__PURE__*/ tuiComponent("TestRenderer");
