// @flow
//
// An avatar: three states, and an `alt` that is empty on purpose.
//
// This one looks like a rounded `<img>` with a `<span>` behind it, and it is a
// component because of two things that are not visible in a screenshot: what
// happens between the states, and what a screen reader says.
//
// # The three states, and the flash between two of them
//
// An image is *loading*, *loaded* or *failed*, and the naive version has two —
// there is an image or there is not — so it renders the fallback whenever the
// image has not painted yet. On a cached image that is a flash of somebody's
// initials for one frame, on every navigation, for ever. The fix is to hold the
// fallback back for a moment: an image that is going to appear immediately does
// so before the delay is up, and only an image that is genuinely slow or
// genuinely broken ever shows initials.
//
// # Asking the DOM, and the one direction it may be asked in
//
// The event is not enough on its own. A cached image can finish loading before
// React has attached `onLoad` — most obviously when the markup came from a
// server and the browser started the request while the JavaScript was still
// downloading — and a component that waits for an event that already happened
// waits for ever, which is the same flash held permanently.
//
// So the element is asked directly, in an effect, which is where this package
// is allowed to read the document (see `index.js`). And it is asked in *one*
// direction:
//
//   * `complete` **and** `naturalWidth > 0` means it loaded. Pixels exist;
//     nothing else produces them.
//   * anything else means nothing. In particular `complete` with no pixels is
//     *not* read as a failure, because that is also what a DOM that does not
//     fetch images says about a perfectly good `src` — a test environment, a
//     server-side render, a browser with images turned off — and concluding
//     "failed" from it would put a fallback over an image that was never asked
//     for. The `error` event is what says a load failed, and an image that
//     failed before anything was listening falls through to the delay and shows
//     the fallback a moment later, which is the same answer arriving late
//     rather than the wrong answer arriving early.
//
// # `alt=""` is the default, and it is the accessible answer
//
// An avatar almost always sits beside the name of the person it is a picture
// of. Putting that name in `alt` makes every screen reader say it twice —
// "Ada Lovelace, image, Ada Lovelace" — which is the most common avatar bug
// there is, and it is caused by a component being helpful. An empty `alt` takes
// the image out of the accessibility tree, which is what "decorative" means and
// what this one is.
//
// A caller whose avatar is the *only* thing identifying the person — a bare
// grid of faces, a comment with no byline — passes `alt` and gets it. The
// default is the common case; the prop is the honest one.
//
// The fallback's content is the caller's and is announced, because this
// component cannot know whether "AL" is a decoration beside a name or the only
// thing on the row. A decorative avatar wants `aria-hidden` on its fallback for
// the same reason its image wants `alt=""`, and that is one prop the caller
// spreads.

"use client";

import * as React from "@uniflowed/react";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "@uniflowed/react";
import { useTimeout } from "@uniflowed/hooks/timing";

import type { RenderProp, Rest } from "./internal/merge-props.js";
import {
  composeHandlers,
  composeRefs,
  withProps,
  withoutComposed,
} from "./internal/merge-props.js";

/**
 * Where an avatar's image is between having been asked for and being there.
 *
 * Three members rather than a `loaded` boolean, because the fallback's whole
 * job is to tell the middle one from the last one: an image that has not
 * arrived *yet* must not be replaced, and one that is never arriving must.
 */
export type AvatarStatus = "loading" | "loaded" | "error";

/** What has been decided, and about which source. */
type Seen = {| readonly source: string | null, readonly status: AvatarStatus |};

/** Nothing asked yet. A source of `null` with no verdict cannot collide. */
const START: Seen = Object.freeze({ source: null, status: "loading" });

type AvatarState = {|
  readonly status: AvatarStatus,
  readonly hasImage: boolean,
  readonly report: (source: string | null, status: AvatarStatus) => void,
  readonly registerImage: (present: boolean) => void,
|};

const AvatarContext: React.Context<AvatarState | null> = createContext(null);

/**
 * The avatar a part belongs to.
 *
 * Raising rather than returning null, for `field.js`'s reason: an
 * `Avatar.Fallback` outside a root would render initials that never go away and
 * would look exactly like one that works.
 */
hook useAvatar(part: string): AvatarState {
  const state = useContext(AvatarContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside an Avatar.Root`);
  }
  return state;
}

/**
 * The avatar, and the state machine its two parts read.
 *
 * A `<span>` rather than a `<div>`, because an avatar belongs beside a name —
 * in a table cell, in a paragraph, inside a button's label — and a block
 * element is invalid in half of those.
 */
export component AvatarRoot(children: React.Node, render?: RenderProp, ...rest: Rest) {
  const [seen, setSeen] = useState<Seen>(START);
  const [hasImage, setHasImage] = useState(false);

  const report = useCallback((source: string | null, status: AvatarStatus) => {
    setSeen((current) => {
      if (current.source !== source) {
        return { source, status };
      }
      // A verdict already reached about this source is not revisited. Without
      // this the failed image would be put back to ask again, fail again, and
      // be put back again: `Avatar.Image` stops rendering the element once it
      // has failed, so "loading" for the same source is a loop and not a retry.
      if (status === "loading" || current.status === status) {
        return current;
      }
      return { source, status };
    });
  }, []);

  const state = useMemo(
    () => ({
      status: seen.status,
      hasImage,
      report,
      registerImage: setHasImage,
    }),
    [seen, hasImage, report],
  );
  const props = withProps(rest, { children });

  return (
    <AvatarContext.Provider value={state}>
      {render != null ? render(props) : <span {...props} />}
    </AvatarContext.Provider>
  );
}

/**
 * The picture.
 *
 * `src` is named rather than left in `rest` because the state machine is about
 * it: a new source is a new question, and the verdict reached about the last
 * one has to stop applying the moment it changes.
 *
 * `alt` defaults to `""`, which is the decision this component exists for as
 * much as the three states are — see the module header.
 *
 * The element stops being rendered once it has failed, rather than being left
 * to show the browser's broken-image glyph beside the fallback that replaced
 * it. That is a thing a package which ships no styles cannot leave to a
 * stylesheet: `hidden` loses to any `display` the caller sets, and a caller who
 * has not written that rule yet would see both.
 */
export component AvatarImage(
  alt?: string = "",
  src?: string | null,
  render?: RenderProp,
  ...rest: Rest
) {
  const avatar = useAvatar("Avatar.Image");
  const element = useRef<HTMLImageElement | null>(null);
  const report = avatar.report;
  const registerImage = avatar.registerImage;
  const source = src ?? null;
  const passed = withoutComposed(rest, ["onError", "onLoad", "ref"]);

  useEffect(() => {
    registerImage(true);
    return () => registerImage(false);
  }, [registerImage]);

  useEffect(() => {
    if (source == null || source === "") {
      // No source is not a slow source. There is nothing coming, so the
      // fallback is the answer now rather than after the delay.
      report(source, "error");
      return;
    }
    const image = element.current;
    // Null only when this source has already failed and the element went with
    // it, in which case the verdict on record is the right one.
    if (image != null && image.complete && image.naturalWidth > 0) {
      report(source, "loaded");
      return;
    }
    report(source, "loading");
  }, [report, source]);

  if (avatar.status === "error") {
    return null;
  }
  const props = withProps(passed, {
    alt,
    onError: composeHandlers(rest.onError, () => report(source, "error")),
    onLoad: composeHandlers(rest.onLoad, () => report(source, "loaded")),
    ref: composeRefs(rest.ref, (node: HTMLImageElement | null) => {
      element.current = node;
    }),
    src: source ?? undefined,
  });
  if (render != null) {
    return render(props);
  }
  return <img {...props} />;
}

/**
 * What is shown instead: initials, a silhouette, a coloured disc.
 *
 * Absent while the image is still loading, which is the point. `delay` is how
 * long "still loading" is allowed to last before the fallback appears anyway —
 * long enough that a cached image never flashes initials, short enough that a
 * genuinely slow one does not leave a hole. Pass `0` to show it the moment
 * there is nothing to show instead.
 *
 * The delay applies to *loading* and to nothing else. A failed image and an
 * avatar with no `Avatar.Image` at all are both answers rather than waits, and
 * the fallback for either is immediate.
 */
export component AvatarFallback(
  children: React.Node,
  delay?: number = 300,
  render?: RenderProp,
  ...rest: Rest
) {
  const avatar = useAvatar("Avatar.Fallback");
  const [elapsed, setElapsed] = useState(false);
  const waiting = avatar.hasImage && avatar.status === "loading";

  useTimeout(() => setElapsed(true), waiting && delay > 0 ? delay : null);

  useEffect(() => {
    if (!waiting) {
      setElapsed(false);
    }
  }, [waiting]);

  if (avatar.status === "loaded" || (waiting && delay > 0 && !elapsed)) {
    return null;
  }
  const props = withProps(rest, { children });
  if (render != null) {
    return render(props);
  }
  return <span {...props} />;
}
