// @flow
//
// A loading placeholder, and the reader it is usually invisible to.
//
// A skeleton is the one component on the presentational list that silently
// makes a page *worse*. A screen of grey rounded rectangles tells a sighted
// reader that content is coming and something is happening. To everybody else
// it is a screen of empty `<div>`s: nothing is announced, nothing is described,
// and the honest summary of the page is that it has no content — which is
// exactly the conclusion somebody reaches before they leave.
//
// Three attributes fix it and none of them is on the grey box:
//
//   * the skeletons themselves are **`aria-hidden="true"`**, because a
//     placeholder is a picture of content and not content;
//   * the region they stand in is **`aria-busy="true"`**, which is the
//     attribute that says "this is being filled in" and stops assistive
//     technology reporting a half-built subtree;
//   * and something has to **say so out loud**, because `aria-busy` is a
//     property a reader can ask about rather than an announcement they are
//     given.
//
// # The live region, and why it is empty for one commit
//
// This is ubugeeei-prod/uf#289's rule met at the worst possible moment.
// `combobox.js` states it: a live region added to the page in the same commit
// as the text it holds is usually not announced, because the technology
// watching it had nothing to watch until it was already too late.
//
// A skeleton screen is busy on its *first* render. So the naive version —
// render `<div role="status">Loading…</div>` while `pending` — mounts the
// region with the sentence already in it and is silent, then unmounts the whole
// thing when the content arrives and is silent again. It announces nothing,
// ever, which is the same as not having been written.
//
// `Skeleton.Root` therefore renders the region empty and fills it in an effect,
// one commit later. The region existed before the text did, which is the whole
// of what the rule asks for, and it costs a second commit on mount and nothing
// afterwards.
//
// # Keep the root mounted across the load
//
// Which is the one thing this component asks of a caller, and the reason
// `busy` is a prop rather than the root's presence. A root that is unmounted
// when the content arrives takes its live region with it, so "loaded" is said
// to nobody and the reader is left with the last thing they heard, which was
// "loading". Wrap the thing that loads and toggle `busy`; that is also what
// lets `aria-busy` go from true to false on one element, which is what it is
// for.
//
// # Not `Progress`
//
// A skeleton says *that* something is loading. `Progress` says *how far along*
// it is, has `aria-valuenow` and lives in `progress.js`. A skeleton with a
// percentage is a progress bar that has been drawn as boxes, and a progress bar
// with no number is the indeterminate one that module already ships.

"use client";

import * as React from "@uniflowed/react";
import { useEffect, useRef, useState } from "@uniflowed/react";

import type { RenderProp, Rest } from "./internal/merge-props.js";
import { withProps } from "./internal/merge-props.js";

/**
 * The region that is being filled in, and the sentence that says so.
 *
 * `children` is the skeletons while `busy`, and the real content once it is
 * not — both go inside, because it is one region either way and `aria-busy`
 * describes it in both states.
 *
 *     <Skeleton.Root busy={pending}>
 *       {pending ? (
 *         <>
 *           <Skeleton.Box />
 *           <Skeleton.Box />
 *         </>
 *       ) : (
 *         <Invoices rows={invoices} />
 *       )}
 *     </Skeleton.Root>
 *
 * `label` and `doneLabel` are what is announced. English defaults, because a
 * component that announces nothing by default is the component this one exists
 * to replace; a real application passes its translation.
 *
 * `doneLabel` is announced only after a spell of `busy`, so a region that was
 * never loading never says it has loaded.
 *
 * `render` changes the element that owns the busy state. The live region stays
 * beside it, mounted by this component, because that timing is the accessibility
 * contract rather than markup the caller can safely recreate by sight.
 */
export component SkeletonRoot(
  children: React.Node,
  busy?: boolean = true,
  label?: string = "Loading…",
  doneLabel?: string = "Loaded",
  render?: RenderProp,
  ...rest: Rest
) {
  const [message, setMessage] = useState("");
  // Whether there has been anything to finish. Written and read in effects
  // only, and nothing renders it — the promise `index.js` makes about refs.
  const waited = useRef(false);

  useEffect(() => {
    if (busy) {
      waited.current = true;
      setMessage(label);
      return;
    }
    setMessage(waited.current ? doneLabel : "");
  }, [busy, doneLabel, label]);

  const props = withProps(rest, {
    "aria-busy": busy ? "true" : undefined,
    children,
  });

  return (
    <>
      {render == null ? <div {...props} /> : render(props)}
      {/*
        Beside the region rather than inside it, so a reader walking into the
        content does not find a sentence about it sitting among the rows — and
        mounted from the first render holding nothing, because a live region
        that appears together with its text is not announced at all. The module
        header says why that matters more here than anywhere else.
      */}
      <div aria-atomic="true" aria-live="polite" data-uf-skeleton-status="" role="status">
        {message}
      </div>
    </>
  );
}

/**
 * One grey box.
 *
 * `aria-hidden="true"`, which is the entire component: a placeholder is a
 * picture of content, and content it is not. Everything about its size, its
 * shape and its shimmer is a class name the caller brings.
 *
 * `children` is allowed and is hidden with the rest of it, because sizing a
 * box by putting the text it stands in for inside it is a real technique and
 * there is no reason to make a caller reach for a second element to do it.
 *
 * `render` changes the placeholder element, not the fact that it is hidden
 * from the accessibility tree.
 */
export component SkeletonBox(children?: React.Node, render?: RenderProp, ...rest: Rest) {
  const props = withProps(rest, { "aria-hidden": "true", children });
  if (render != null) {
    return render(props);
  }
  return <div {...props} />;
}
