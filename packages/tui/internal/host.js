// @flow
//
// The React binding: what React mutates, and how a frame comes out of it.
//
// # Internal to `@uniflowed/tui`
//
// Absent from `package.json#exports`. A renderer holds one React root and one
// terminal's worth of state, and every operation here assumes it is the only
// one touching them: a second `createRoot` over the same renderer, or a
// `pressKey` that bypasses the driver, produces a frame the terminal is never
// told about. `render` and `testRender` in `terminal.js` are the two supported
// ways in.
//
// A renderer here is a plain object holding a node tree, the terminal's
// capabilities, its size, and the frame last drawn. React never sees any of
// that. It sees a *host config* — a table of about thirty functions that say
// how to make a node, put a node inside another, and change a node's props —
// and it does the rest. `createInstance`, `appendChild`, `removeChild`: the
// whole binding is those, and the reason it is worth writing rather than
// avoiding is that everything above it is then real React. Hooks, effects,
// context, Suspense, `memo`, the React Compiler's output — all of it works
// because none of it knows the host is a terminal.
//
// # Why `react-reconciler` and not a small reconciler of our own
//
// The alternative was to walk the element tree, call the function components
// and interpret what they return, which is about two hundred lines and looks
// tempting until the second question: what does `useState` do? A component
// model without hooks is not React, and the moment hooks are added the two
// hundred lines are a second React with its own bugs — which is precisely what
// `ubugeeei-redundancy.md` means by not building approximate replacements for
// upstream semantics.
//
// `react-reconciler` is the package React ships for this, and React Native,
// Ink and react-three-fiber are all built on it. It is versioned separately
// and its own README calls it experimental, which is a real cost and is
// recorded here rather than discovered at the next React minor: this package
// pins `^0.33.0`, which peers on React 19.2, and a React upgrade is a change
// that has to be tested against this binding.
//
// # Rendering is synchronous, deliberately
//
// React's concurrent scheduler exists to keep a browser's main thread
// responsive while it renders — to let a 60 Hz paint and a user's typing
// interrupt a long tree. A terminal has neither problem and one the browser
// does not: a keystroke must produce a frame *now*, because the reader is
// looking at a cursor that has not moved yet. So key dispatch runs at
// `DiscreteEventPriority` and the frame is flushed synchronously afterwards,
// which is what `react-dom` does for a click for the same reason.
//
// This also makes tests deterministic without `act()` and without waiting on
// timers: press a key, read the frame.

import * as React from "@uniflowed/react";
import Reconciler from "react-reconciler";
// `constants.js` with the extension: `react-reconciler` ships no `exports`
// map, so Node resolves its subpaths as plain files and an extensionless
// specifier fails only under ESM — which is to say, only in the runtime this
// package actually runs in.
import { DefaultEventPriority, DiscreteEventPriority } from "react-reconciler/constants.js";

import type { Capabilities } from "../capability.js";
import type { Frame, Rect } from "../cells.js";
import { createFrame } from "../cells.js";
import type { Update } from "../diff.js";
import { diffFrames } from "../diff.js";
import type { KeyEvent } from "../keys.js";
import { layout } from "../layout.js";
import type { MouseEvent } from "../mouse.js";
import { MouseButton, derive } from "../mouse.js";
import type { HitGrid } from "./hits.js";
import { createHitGrid, hitAt } from "./hits.js";
import { measureText, paint, wrapModeOf } from "./paint.js";
import type { TuiNode, TuiProps } from "./tree.js";
import { applyProps, createNode } from "./tree.js";

/** A global key handler, as `useKeyboard` registers one. */
export type KeyHandler = (key: KeyEvent) => void;

/**
 * Everything one mounted application owns.
 *
 * Mutable, and owned by exactly one React root. The fields React writes
 * (`root`) and the fields the terminal writes (`width`, `height`) are
 * deliberately in the same object: a resize has to invalidate the previous
 * frame, and putting the two in separate places is how a renderer ends up
 * diffing an 80-column frame against a 120-column one.
 */
export type Renderer = {
  root: TuiNode,
  capabilities: Capabilities,
  width: number,
  height: number,
  /** The frame currently on the terminal, or `null` before the first draw. */
  previous: Frame | null,
  /** Global key handlers, in registration order, as OpenTUI orders them. */
  keyHandlers: Array<KeyHandler>,
  /** Called after every commit, so a driver knows to draw. */
  onCommit: (() => void) | null,
  /**
   * The current size, as one object that is replaced rather than mutated.
   *
   * `useSyncExternalStore` requires a snapshot that is referentially stable
   * between changes — it compares the value it is given with `Object.is` and
   * re-renders forever if a fresh object comes back each time. Keeping the
   * snapshot here, and replacing it only in `resize`, is what makes that
   * true; a `getSnapshot` that returned `{ width, height }` would be the
   * infinite-loop bug that hook's documentation warns about.
   */
  size: { readonly width: number, readonly height: number },
  /** Who to tell when the terminal is resized. */
  sizeListeners: Set<() => void>,
  /**
   * Whether this renderer routes mouse reports.
   *
   * False by default, and the hit grid is not built when it is: a keyboard
   * application should not pay a per-frame cost for a device it never reads.
   * `terminal.js` sets it from `render`'s `mouse` option, which is also what
   * decides whether the terminal is asked to report the mouse at all — the two
   * must agree, or an application receives reports it has no grid to route.
   */
  mouseEnabled: boolean,
  /** Which node owned each cell of the last frame, or `null`. */
  hits: HitGrid | null,
  /** The node the pointer was last over, so `over`/`out` can be derived. */
  hovered: TuiNode | null,
  /** The node a left-button drag started on, while one is in progress. */
  dragSource: TuiNode | null,
  /** Whether that press has actually moved yet: a click is not a drag. */
  dragging: boolean,
};

/** A renderer that draws into a `width` by `height` rectangle. */
export function createRenderer(
  width: number,
  height: number,
  capabilities: Capabilities,
  mouseEnabled: boolean = false,
): Renderer {
  return {
    root: createNode("root", {}),
    capabilities,
    width,
    height,
    previous: null,
    keyHandlers: [],
    onCommit: null,
    size: { width, height },
    sizeListeners: new Set(),
    mouseEnabled,
    hits: null,
    hovered: null,
    dragSource: null,
    dragging: false,
  };
}

/** The context every hook in this package reads to find its renderer. */
export const RendererContext: React.Context<Renderer | null> = React.createContext<Renderer | null>(
  null,
);

/**
 * Lay the tree out and paint it.
 *
 * Produces a new frame each time rather than mutating the last one, because
 * the last one is what the diff compares against: painting over it would make
 * every frame identical to its predecessor and the terminal would never
 * change. The allocation is one frame per draw, which is the one allocation
 * this design cannot avoid.
 */
export function renderFrame(renderer: Renderer): Frame {
  const { root, width, height } = renderer;
  layout(root, 0, 0, width, height);
  const frame = createFrame(width, height);
  const full: Rect = { x: 0, y: 0, width, height };
  // The hit grid belongs to the frame that produced it, so it is replaced
  // whole rather than updated. A grid kept from an earlier frame would route a
  // click to a node that has moved, which is the bug that makes a terminal
  // menu act on the row above the one that was clicked.
  const hits = renderer.mouseEnabled ? createHitGrid(width, height) : null;
  paint(root, frame, renderer.capabilities, full, hits);
  if (hits != null) {
    renderer.hits = hits;
  }
  return frame;
}

/**
 * Draw the next frame and report what it costs to put on the terminal.
 *
 * Advances `previous`, so a caller that does not write the returned bytes has
 * lied to the renderer about what is on the screen. That is why this returns
 * the bytes instead of writing them: the one place that knows how to write to
 * a terminal is `terminal.js`, and the one place that knows what to write is
 * here.
 */
export function nextUpdate(renderer: Renderer): Update {
  const frame = renderFrame(renderer);
  const update = diffFrames(renderer.previous, frame, renderer.capabilities);
  renderer.previous = frame;
  return update;
}

/**
 * The node that currently has focus, or `null`.
 *
 * Focus is declarative: a node has it when its props say `focused`, which
 * makes it ordinary React state and means an application moves focus the same
 * way it changes anything else. This is OpenTUI's model — it has no automatic
 * Tab traversal either — and the reason to follow it rather than to add
 * traversal is that "what does Tab do" is an application's question. In a form
 * it moves to the next field; in an editor it inserts a tab.
 *
 * The *first* such node wins when an application marks two, rather than the
 * last or an error. A terminal renderer that throws because a state update
 * briefly marked two fields focused is a renderer that crashes during the one
 * frame between "blur that" and "focus this".
 */
export function focusedNode(renderer: Renderer): TuiNode | null {
  let found: TuiNode | null = null;
  const walk = (node: TuiNode) => {
    if (found != null) {
      return;
    }
    if (node.type === "box" && node.props.focused === true && node.props.focusable !== false) {
      found = node;
      return;
    }
    for (const child of node.children) {
      walk(child);
    }
  };
  walk(renderer.root);
  return found;
}

/**
 * Deliver one key press.
 *
 * Global handlers first, in registration order, then the focused node — which
 * is OpenTUI's order, and the reason a global quit key works even while a text
 * input has focus. The two ways to interrupt that are not severities of one
 * another and are documented on `KeyEvent`.
 */
export function dispatchKey(renderer: Renderer, key: KeyEvent): void {
  for (const handler of renderer.keyHandlers.slice()) {
    handler(key);
    if (key.propagationStopped) {
      return;
    }
  }
  if (key.defaultPrevented) {
    return;
  }
  const target = focusedNode(renderer);
  if (target == null) {
    return;
  }
  const handler = target.props.onKeyDown;
  if (typeof handler === "function") {
    handler(key);
  }
}

/**
 * The prop each mouse event type is delivered through.
 *
 * OpenTUI's handler names, exactly: a component copied from its interaction
 * page finds its handler called here. `onMouse` is not in this table because
 * it is called for every type, after the specific one.
 */
const MOUSE_HANDLERS: { readonly [string]: string } = {
  down: "onMouseDown",
  up: "onMouseUp",
  move: "onMouseMove",
  drag: "onMouseDrag",
  "drag-end": "onMouseDragEnd",
  drop: "onMouseDrop",
  over: "onMouseOver",
  out: "onMouseOut",
  scroll: "onMouseScroll",
};

/** The `id` a box was given, which is the only name an event can carry. */
function nodeId(node: TuiNode | null): string | null {
  if (node == null) {
    return null;
  }
  const id = node.props.id;
  return typeof id === "string" ? id : null;
}

/**
 * Whether a node is still part of the tree this renderer draws.
 *
 * A hovered node and a drag source are held across events, and React can
 * unmount either of them in between — a menu that closes while the pointer is
 * over it, a list row that a state update removed. Delivering `out` or
 * `drag-end` to a node that has left the tree is the same leak `useKeyboard`
 * avoids by unsubscribing: a component that is gone acts on an event about a
 * screen the reader has left.
 */
function attached(renderer: Renderer, node: TuiNode): boolean {
  let current: TuiNode | null = node;
  while (current != null) {
    if (current === renderer.root) {
      return true;
    }
    current = current.parent;
  }
  return false;
}

/**
 * Deliver one mouse event to a node, then to its ancestors.
 *
 * OpenTUI's propagation: the event starts at a node and bubbles up the parent
 * chain until something calls `stopPropagation()` or the root is reached.
 * There is no capture phase — OpenTUI documents one direction, and a phase
 * nothing can register for would be a field in an event rather than a feature.
 *
 * `currentTarget` is rewritten at each step and `target` is not, which is the
 * DOM's rule and the reason both exist: a panel's handler needs to know that
 * the click was on the button inside it.
 */
function bubble(
  from: TuiNode,
  event: MouseEvent,
  target: TuiNode | null,
  source: TuiNode | null,
): void {
  event.target = nodeId(target);
  event.source = nodeId(source);
  let current: TuiNode | null = from;
  while (current != null) {
    if (current.type === "box") {
      event.currentTarget = nodeId(current);
      const specific = current.props[MOUSE_HANDLERS[event.type]];
      if (typeof specific === "function") {
        specific(event);
      }
      const catchAll = current.props.onMouse;
      if (typeof catchAll === "function") {
        catchAll(event);
      }
      if (event.propagationStopped) {
        return;
      }
    }
    current = current.parent;
  }
}

/**
 * Deliver one mouse report.
 *
 * The node under the pointer comes from the hit grid the last paint recorded,
 * so it is the node a reader can *see* there rather than the node whose
 * geometry contains the point — those differ under `overflow: "hidden"` and
 * inside a `ScrollBox`, which is most of the reason the grid exists.
 *
 * Three things happen here that a terminal does not report and OpenTUI
 * specifies:
 *
 * * **`over` and `out`.** A terminal reports positions; a hover is a change of
 *   topmost node, so it is computed by comparing this report's node with the
 *   last one's. They are delivered before the report that caused them, so that
 *   a handler which highlights on `over` has already run when the `down` that
 *   follows arrives.
 * * **Drag capture.** A left press remembers the node it landed on, and every
 *   later motion goes to *that* node rather than to whatever is under the
 *   pointer now. Without it, dragging a slider's handle stops working the
 *   moment the pointer leaves the handle — which is every drag.
 * * **The release.** A press that never moved is a click and produces one
 *   `up`. A press that did produces `drag-end` and `up` at the source, then
 *   `drop` at whatever is under the pointer carrying `event.source`, and an
 *   `up` there too unless that is the source again — one release is one `up`
 *   per node.
 *
 * A renderer with `mouseEnabled` false has no grid and drops the report. That
 * is not a silent failure to guard against: nothing turns mouse reporting on
 * in the terminal either, so a report can only arrive from a caller who
 * assembled one by hand.
 */
export function dispatchMouse(renderer: Renderer, event: MouseEvent): void {
  if (renderer.hits == null) {
    // A report before the first draw. `render` draws immediately after
    // mounting, so this is the in-memory renderer's path: a test that presses
    // the mouse before it asks for a frame.
    renderFrame(renderer);
  }
  const grid = renderer.hits;
  if (grid == null) {
    return;
  }

  const hit = hitAt(grid, event.x, event.y);
  if (hit !== renderer.hovered) {
    const left = renderer.hovered;
    renderer.hovered = hit;
    if (left != null && attached(renderer, left)) {
      bubble(left, derive(event, "out"), left, renderer.dragSource);
    }
    if (hit != null) {
      bubble(hit, derive(event, "over"), hit, renderer.dragSource);
    }
  }

  if (event.type === "down") {
    if (event.button === MouseButton.LEFT) {
      renderer.dragSource = hit;
      renderer.dragging = false;
    }
    if (hit != null) {
      bubble(hit, event, hit, null);
    }
    return;
  }

  if (event.type === "drag") {
    const source = renderer.dragSource;
    if (source != null && attached(renderer, source)) {
      renderer.dragging = true;
      bubble(source, event, hit, source);
      return;
    }
    if (hit != null) {
      bubble(hit, event, hit, null);
    }
    return;
  }

  if (event.type === "up") {
    const source = renderer.dragSource;
    const dragged = renderer.dragging;
    renderer.dragSource = null;
    renderer.dragging = false;
    if (source != null && dragged && attached(renderer, source)) {
      // Each of these is its own event object: they are four separate
      // deliveries, and one handler calling `stopPropagation()` must not
      // silence the next node's.
      bubble(source, derive(event, "drag-end"), hit, source);
      bubble(source, derive(event, "up"), hit, source);
      if (hit != null) {
        bubble(hit, derive(event, "drop"), hit, source);
        if (hit !== source) {
          bubble(hit, derive(event, "up"), hit, source);
        }
      }
      return;
    }
    if (hit != null) {
      bubble(hit, event, hit, null);
    }
    return;
  }

  if (hit != null) {
    bubble(hit, event, hit, renderer.dragSource);
  }
}

/**
 * The current update priority.
 *
 * React asks for this to decide which lane an update belongs to. It is module
 * state rather than renderer state because React asks without saying which
 * renderer it is asking for, and a process draws one terminal.
 */
let currentPriority: number = DefaultEventPriority;

const noop = () => {};

const hostConfig = {
  supportsMutation: true,
  supportsPersistence: false,
  supportsHydration: false,
  supportsMicrotasks: true,
  isPrimaryRenderer: true,
  warnsIfNotActing: true,
  noTimeout: -1,
  scheduleTimeout: setTimeout,
  cancelTimeout: clearTimeout,
  scheduleMicrotask: queueMicrotask,
  rendererPackageName: "@uniflowed/tui",
  rendererVersion: "0.0.0-alpha.5",

  getRootHostContext: (): {} => ({}),
  getChildHostContext: (parent: {}): {} => parent,
  getPublicInstance: (instance: TuiNode): TuiNode => instance,
  prepareForCommit: (): null => null,
  resetAfterCommit: (renderer: Renderer): void => {
    // The picture has changed, so what is under the pointer may have. The grid
    // is dropped rather than rebuilt: the next draw builds one anyway, and a
    // mouse report that arrives before that draw builds its own. Keeping it
    // would route the click after a state update by the frame before it — a
    // menu that moved under the pointer acting on the row it used to show.
    renderer.hits = null;
    if (renderer.onCommit != null) {
      renderer.onCommit();
    }
  },
  preparePortalMount: noop,

  createInstance: (type: string, props: TuiProps): TuiNode => {
    const node = createNode(type === "uf-text" ? "text" : "box", props);
    if (node.type === "text") {
      // A text node is the only leaf that knows its own size, and it only
      // knows it once it is told how wide it may be. This is Yoga's measure
      // callback; `layout.js` calls it and never learns what text is.
      node.measure = (available: number) => measureText(node, available, wrapModeOf(node));
    }
    return node;
  },
  createTextInstance: (text: string): TuiNode => {
    const node = createNode("chars", {});
    node.text = text;
    return node;
  },
  appendInitialChild: (parent: TuiNode, child: TuiNode): void => {
    child.parent = parent;
    parent.children.push(child);
  },
  finalizeInitialChildren: (): boolean => false,
  // Never: a `<Text>`'s children are nodes, because React has to be able to
  // move and replace them individually. Answering `true` here would collapse
  // them into one string and lose the styles the nested ones carry.
  shouldSetTextContent: (): boolean => false,
  clearContainer: (renderer: Renderer): void => {
    renderer.root.children = [];
  },

  appendChild: (parent: TuiNode, child: TuiNode): void => {
    child.parent = parent;
    parent.children.push(child);
  },
  appendChildToContainer: (renderer: Renderer, child: TuiNode): void => {
    child.parent = renderer.root;
    renderer.root.children.push(child);
  },
  insertBefore: (parent: TuiNode, child: TuiNode, before: TuiNode): void => {
    child.parent = parent;
    remove(parent.children, child);
    const at = parent.children.indexOf(before);
    parent.children.splice(at < 0 ? parent.children.length : at, 0, child);
  },
  insertInContainerBefore: (renderer: Renderer, child: TuiNode, before: TuiNode): void => {
    hostConfig.insertBefore(renderer.root, child, before);
  },
  removeChild: (parent: TuiNode, child: TuiNode): void => {
    remove(parent.children, child);
    child.parent = null;
  },
  removeChildFromContainer: (renderer: Renderer, child: TuiNode): void => {
    remove(renderer.root.children, child);
    child.parent = null;
  },
  commitUpdate: (node: TuiNode, _type: string, _previous: TuiProps, next: TuiProps): void => {
    applyProps(node, next);
  },
  commitTextUpdate: (node: TuiNode, _previous: string, next: string): void => {
    node.text = next;
  },
  resetTextContent: noop,
  commitMount: noop,
  // Hiding is how React implements a Suspense fallback and `<Activity>`. A
  // hidden node keeps its place in the tree and draws nothing, which layout
  // reads as a zero-size node rather than as an absent one.
  hideInstance: (node: TuiNode): void => {
    applyProps(node, { ...node.props, width: 0, height: 0, hidden: true });
  },
  unhideInstance: (node: TuiNode, props: TuiProps): void => {
    applyProps(node, props);
  },
  hideTextInstance: (node: TuiNode): void => {
    node.text = "";
  },
  unhideTextInstance: (node: TuiNode, text: string): void => {
    node.text = text;
  },
  detachDeletedInstance: noop,

  getCurrentUpdatePriority: (): number => currentPriority,
  setCurrentUpdatePriority: (priority: number): void => {
    currentPriority = priority;
  },
  resolveUpdatePriority: (): number =>
    currentPriority === 0 ? DefaultEventPriority : currentPriority,
  shouldAttemptEagerTransition: (): boolean => false,
  requestPostPaintCallback: noop,
  maySuspendCommit: (): boolean => false,
  preloadInstance: (): boolean => true,
  startSuspendingCommit: noop,
  suspendInstance: noop,
  waitForCommitToBeReady: (): null => null,
  NotPendingTransition: null,
  HostTransitionContext: React.createContext(null),
  resetFormInstance: noop,
  trackSchedulerEvent: noop,
  resolveEventType: (): null => null,
  resolveEventTimeStamp: (): number => -1.1,
  beforeActiveInstanceBlur: noop,
  afterActiveInstanceBlur: noop,
  prepareScopeUpdate: noop,
  getInstanceFromScope: (): null => null,
  getInstanceFromNode: (): null => null,
};

function remove(children: Array<TuiNode>, child: TuiNode): void {
  const at = children.indexOf(child);
  if (at >= 0) {
    children.splice(at, 1);
  }
}

const reconciler = Reconciler(hostConfig);

/** A mounted React tree, and the two things a caller does with one. */
export type Root = {
  /** Render an element into this renderer, synchronously. */
  render(node: React.Node): void,
  /** Unmount it, running every effect cleanup. */
  unmount(): void,
};

/**
 * Mount React into a renderer.
 *
 * The renderer is not owned by the root: unmounting the tree leaves the
 * renderer able to draw the empty frame, and tearing the terminal down is the
 * job of whoever set it up. That is OpenTUI's split between `root.unmount()`
 * and `renderer.destroy()`, and it exists because the two failure paths are
 * different — a component that throws should not leave a terminal in raw mode.
 */
export function createRoot(renderer: Renderer): Root {
  const container = reconciler.createContainer(
    renderer,
    // A concurrent root, driven synchronously. The alternative — a legacy
    // root — also renders synchronously but opts out of every React 19
    // behaviour that is tested against concurrent roots, which is a strange
    // thing for a new renderer to inherit.
    1,
    null,
    false,
    null,
    "uf-tui",
    (error: mixed) => {
      throw error;
    },
    noop,
    noop,
    null,
  );

  const flush = () => {
    // Passive effects can schedule more work — a `useEffect` that sets state
    // is how an application reacts to being mounted — so this settles rather
    // than flushing once. The bound is not a safety net for a well-written
    // application; it is what turns an effect loop into a clear failure
    // instead of a terminal that stops responding.
    for (let pass = 0; pass < 50; pass += 1) {
      reconciler.flushSyncWork();
      if (!reconciler.flushPassiveEffects()) {
        return;
      }
    }
    throw new Error(
      "@uniflowed/tui: an effect kept scheduling work after 50 passes; " +
        "a `useEffect` is setting state that re-triggers it.",
    );
  };

  return {
    render(node: React.Node) {
      withPriority(DiscreteEventPriority, () => {
        reconciler.updateContainerSync(node, container, null, null);
        flush();
      });
    },
    unmount() {
      withPriority(DiscreteEventPriority, () => {
        reconciler.updateContainerSync(null, container, null, null);
        flush();
      });
    },
  };
}

/**
 * Run `work` at a given React update priority, and flush what it schedules.
 *
 * This is how a key press becomes a frame before the function returns. React
 * assigns an update to a lane from the priority in effect when `setState` is
 * called, and only the sync lane is flushed by `flushSyncWork()` — so a
 * handler that runs at the default priority schedules work for a later task,
 * and the terminal shows the previous frame until that task runs.
 */
export function withPriority<T>(priority: number, work: () => T): T {
  const previous = currentPriority;
  currentPriority = priority;
  try {
    return work();
  } finally {
    currentPriority = previous;
  }
}

/**
 * Deliver a key and settle everything it caused.
 *
 * The entry point a driver — the real terminal, or a test — uses. It exists so
 * that "press a key" and "the frame that results" are one call rather than a
 * call and a hope.
 */
export function pressKey(renderer: Renderer, key: KeyEvent): void {
  withPriority(DiscreteEventPriority, () => {
    dispatchKey(renderer, key);
    settle();
  });
}

/**
 * Deliver a mouse report and settle everything it caused.
 *
 * The counterpart of {@link pressKey}, at the same priority and for the same
 * reason: a reader who clicked is looking at a frame that has not changed yet.
 */
export function pressMouse(renderer: Renderer, event: MouseEvent): void {
  withPriority(DiscreteEventPriority, () => {
    dispatchMouse(renderer, event);
    settle();
  });
}

/**
 * Tell the renderer the terminal is a different size, and settle the redraw.
 *
 * `previous` is discarded rather than kept. A resized terminal has already
 * reflowed whatever was on it — the emulator moved the text itself, in a way
 * this renderer neither performed nor can predict — so the frame it thought
 * was on the screen describes nothing, and diffing against it produces an
 * update that repairs a screen that does not exist. The next draw is a full
 * repaint, which is correct and is the one case where a full repaint is.
 */
export function resize(renderer: Renderer, width: number, height: number): void {
  if (renderer.width === width && renderer.height === height) {
    return;
  }
  renderer.width = width;
  renderer.height = height;
  renderer.size = { width, height };
  renderer.previous = null;
  // The same reasoning as `previous`, one axis further: a grid is a rectangle
  // of the old size, and indexing it with a coordinate from the new one reads
  // the wrong row.
  renderer.hits = null;
  renderer.hovered = null;
  withPriority(DiscreteEventPriority, () => {
    for (const listener of Array.from(renderer.sizeListeners)) {
      listener();
    }
    settle();
  });
}

/** Flush everything React has scheduled, including the effects it runs. */
function settle(): void {
  reconciler.flushSyncWork();
  for (let pass = 0; pass < 50 && reconciler.flushPassiveEffects(); pass += 1) {
    reconciler.flushSyncWork();
  }
}
