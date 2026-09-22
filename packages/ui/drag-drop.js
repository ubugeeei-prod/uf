// @flow
"use client";
import { useState } from "@uniflowed/react";

export type Drop = {| readonly keys: $ReadOnlyArray<string>, readonly target: string |};
export type DragAndDrop = {
  dragging: boolean,
  announcement: string,
  start: (key: string, label: string) => void,
  drop: (target: string, incoming?: $ReadOnlyArray<string>, label?: string) => void,
  cancel: () => void,
  getDragProps: (
    key: string,
    label: string,
  ) => {
    draggable: boolean,
    onDragStart: (event: $FlowFixMe) => void,
    onDragEnd: () => void,
    onKeyDown: (event: $FlowFixMe) => void,
  },
  getDropProps: (
    target: string,
    label?: string,
  ) => {
    onDragOver: (event: $FlowFixMe) => void,
    onDrop: (event: $FlowFixMe) => void,
    onKeyDown: (event: $FlowFixMe) => void,
  },
};
const MIME = "application/x-uf-collection";

/** Pointer drag data and keyboard lift/drop share one validated payload. */
export hook useDragAndDrop(options: {
  onDrop: (drop: Drop) => void,
  disabled?: boolean,
}): DragAndDrop {
  const [keys, setKeys] = useState<$ReadOnlyArray<string>>([]);
  const [announcement, announce] = useState("");
  const cancel = () => {
    setKeys([]);
    announce("Drag cancelled");
  };
  const start = (key: string, label: string) => {
    if (options.disabled) return;
    setKeys([key]);
    announce(`Picked up ${label}. Move to a drop target and press Enter. Escape cancels.`);
  };
  const drop = (
    target: string,
    incoming: $ReadOnlyArray<string> = keys,
    label: string = target,
  ) => {
    if (options.disabled || incoming.length === 0) return;
    options.onDrop({ keys: incoming, target });
    setKeys([]);
    announce(`Dropped on ${label}`);
  };
  return {
    dragging: keys.length > 0,
    announcement,
    start,
    drop,
    cancel,
    getDragProps: (key: string, label: string) => ({
      draggable: !options.disabled,
      onDragStart: (event: $FlowFixMe) => {
        if (options.disabled) {
          event.preventDefault();
          return;
        }
        start(key, label);
        event.dataTransfer.setData(MIME, JSON.stringify([key]));
        event.dataTransfer.setData("text/plain", label);
        event.dataTransfer.effectAllowed = "move";
      },
      onDragEnd: () => setKeys([]),
      onKeyDown: (event: $FlowFixMe) => {
        if (event.key === "Escape" && keys.length > 0) {
          event.preventDefault();
          cancel();
        } else if (event.key === " " && !keys.length) {
          event.preventDefault();
          start(key, label);
        }
      },
    }),
    getDropProps: (target: string, label: string = target) => ({
      onDragOver: (event: $FlowFixMe) => {
        if (!options.disabled && Array.from(event.dataTransfer.types).includes(MIME)) {
          event.preventDefault();
          event.dataTransfer.dropEffect = "move";
        }
      },
      onDrop: (event: $FlowFixMe) => {
        if (options.disabled) return;
        const raw = event.dataTransfer.getData(MIME);
        if (raw.length > 64000) return;
        let incoming;
        try {
          incoming = JSON.parse(raw);
        } catch {
          return;
        }
        if (
          !Array.isArray(incoming) ||
          incoming.length === 0 ||
          !incoming.every((key) => typeof key === "string")
        )
          return;
        event.preventDefault();
        drop(target, incoming, label);
      },
      onKeyDown: (event: $FlowFixMe) => {
        if (keys.length === 0) return;
        if (event.key === "Enter") {
          event.preventDefault();
          drop(target, keys, label);
        } else if (event.key === "Escape") {
          event.preventDefault();
          cancel();
        }
      },
    }),
  };
}
