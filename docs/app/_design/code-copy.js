"use client";
// @flow
//
// A copy button on every code sample.
//
// The samples are rendered on the server by the markdown pipeline, so there is
// no component per sample to give a button to. This adds one to each `pre` in
// the article once the page has hydrated — after React has finished with the
// document the server sent, so hydration never sees it — and again whenever a
// navigation puts a new article under the layout. Without JavaScript the
// samples are exactly what they were: selectable text.
//
// It copies the code and nothing else: the `code` element's text, not the
// button's label, and not the `$` a command line draws in front of itself,
// which is CSS for the same reason.

import { useEffect } from "@uniflowed/react";

const MARK = "data-copy";

/** `path` is what says a navigation replaced the article; the component stays mounted. */
export component CodeCopy(path: string) {
  useEffect(() => {
    const article = document.getElementById("content");
    if (article == null) {
      return;
    }
    const added: Array<HTMLButtonElement> = [];
    for (const block of article.querySelectorAll("pre, .command")) {
      if (block.hasAttribute(MARK)) {
        continue;
      }
      const code = block.querySelector("code") ?? block;
      const button = document.createElement("button");
      button.type = "button";
      button.className = "copy";
      button.textContent = "Copy";
      button.setAttribute("aria-label", "Copy this sample");
      button.addEventListener("click", () => {
        void copy(code.textContent, button);
      });
      block.setAttribute(MARK, "");
      block.append(button);
      added.push(button);
    }
    return () => {
      for (const button of added) {
        button.parentElement?.removeAttribute(MARK);
        button.remove();
      }
    };
  }, [path]);

  return null;
}

async function copy(text: string, button: HTMLButtonElement): Promise<void> {
  let label = "Copied";
  try {
    await navigator.clipboard.writeText(text.replace(/\n$/, ""));
  } catch {
    // A page without clipboard permission — an insecure origin, a sandboxed
    // frame — still has selection, so select the text for the reader to copy.
    const selection = window.getSelection();
    const code = button.parentElement?.querySelector("code") ?? button.parentElement;
    if (selection != null && code != null) {
      const range = document.createRange();
      range.selectNodeContents(code);
      selection.removeAllRanges();
      selection.addRange(range);
    }
    label = "Selected";
  }
  button.textContent = label;
  window.setTimeout(() => {
    button.textContent = "Copy";
  }, 1600);
}
