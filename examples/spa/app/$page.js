// @flow
"use client";

import { useState } from "@uniflowed/react";
import { useAtomValue, useSetAtom } from "@uniflowed/state";
import { props, stylex } from "@uniflowed/stylex";

import { focus, tokens } from "./tokens.stylex.js";
import { addTask, clearCompleted, completed, tasks, toggleTask } from "./model.js";

export component Page() {
  const items             = useAtomValue(tasks);
  const done              = useAtomValue(completed);
  const add               = useSetAtom(addTask);
  const toggle            = useSetAtom(toggleTask);
  const clear             = useSetAtom(clearCompleted);
  const [draft, setDraft] = useState("");

  return (
    <>
      <p {...props(styles.eyebrow)}>A little room for today</p>
      <h1 {...props(styles.title)}>Make room for what matters.</h1>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          add(draft);
          setDraft("");
        }}
      >
        <label {...props(styles.label)} htmlFor="task">
          Add a task
        </label>
        <div {...props(styles.compose)}>
          <input
            {...props(styles.field, focus.ring)}
            id="task"
            name="task"
            value={draft}
            maxLength={160}
            autoComplete="off"
            onChange={(event) => setDraft(event.currentTarget.value)}
            placeholder="What is next?"
          />
          <button
            {...props(styles.button, styles.submit, focus.ring)}
            type="submit"
            disabled={draft.trim().length === 0}
          >
            Add task
          </button>
        </div>
      </form>
      <ul {...props(styles.tasks)} aria-label="Tasks">
        {items.map((task) => (
          <li {...props(styles.task)} key={task.id}>
            <label {...props(styles.taskLabel)}>
              <input
                {...props(styles.checkbox, focus.ring)}
                type="checkbox"
                checked={task.done}
                onChange={() => toggle(task.id)}
              />
              <span
                {...props(
                  match (task.done) {
                    true  => styles.done,
                    false => null,
                  },
                )}
              >
                {task.title}
              </span>
            </label>
          </li>
        ))}
      </ul>
      {
        match (items.length === 0) {
          true  => <p {...props(styles.quiet)}>Nothing waiting. Add something when you are ready.</p>,
          false => null,
        }
      }
      <div {...props(styles.summary)}>
        <p {...props(styles.meta)} role="status">
          {done} of {items.length} complete
        </p>
        <button
          {...props(styles.button, styles.subtle, focus.ring)}
          type="button"
          disabled={done === 0}
          onClick={() => clear()}
        >
          Clear completed
        </button>
      </div>
    </>
  );
}

const styles = stylex.create({
  eyebrow: {
    color         : tokens.muted,
    fontSize      : "12px",
    letterSpacing : "0.06em",
    textTransform : "uppercase",
  },
  title: {
    maxWidth     : "15ch",
    margin       : "0 0 2rem",
    fontSize     : "clamp(2rem, 5vw, 3rem)",
    lineHeight   : "1.1",
    letterSpacing: "-0.04em",
  },
  label: {
    fontSize: "14px",
  },
  compose: {
    display : "flex",
    gap     : "0.75rem",
    margin  : "0.5rem 0 2rem",
    "@media (max-width: 420px)": {
      flexWrap: "wrap",
    },
  },
  field: {
    width           : "100%",
    minWidth        : "0",
    padding         : "0.65rem",
    color           : tokens.ink,
    backgroundColor : "transparent",
    borderWidth     : "1px",
    borderStyle     : "solid",
    borderColor     : tokens.rule,
    borderRadius    : "3px",
    font            : "inherit",
  },
  button: {
    padding         : "0.65rem 1rem",
    color           : tokens.paper,
    backgroundColor : tokens.ink,
    borderWidth     : "1px",
    borderStyle     : "solid",
    borderColor     : tokens.ink,
    borderRadius    : "3px",
    whiteSpace      : "nowrap",
    cursor          : "pointer",
    font            : "inherit",
    ":disabled"     : {
      opacity: "0.5",
      cursor : "default",
    },
  },
  submit: {
    "@media (max-width: 420px)": {
      width: "100%",
    },
  },
  tasks: {
    margin        : "0",
    padding       : "0",
    listStyle     : "none",
  },
  task: {
    padding        : "1rem 0",
    borderTopWidth : "1px",
    borderTopStyle : "solid",
    borderTopColor : tokens.rule,
  },
  taskLabel: {
    display   : "flex",
    alignItems: "center",
    gap       : "0.75rem",
    cursor    : "pointer",
    fontSize  : "14px",
  },
  checkbox: {
    width      : "18px",
    height     : "18px",
    margin     : "0",
    accentColor: tokens.accent,
    flexShrink : "0",
    font       : "inherit",
  },
  done: {
    color          : tokens.muted,
    textDecoration : "line-through",
  },
  summary: {
    display        : "flex",
    flexWrap       : "wrap",
    alignItems     : "center",
    justifyContent : "space-between",
    gap            : "1rem",
    paddingTop     : "1rem",
    borderTopWidth : "1px",
    borderTopStyle : "solid",
    borderTopColor : tokens.rule,
  },
  meta: {
    color   : tokens.muted,
    fontSize: "14px",
  },
  quiet: {
    color   : tokens.muted,
    fontSize: "14px",
  },
  subtle: {
    color           : tokens.muted,
    backgroundColor : "transparent",
    borderColor     : tokens.rule,
    fontSize        : "14px",
  },
});
