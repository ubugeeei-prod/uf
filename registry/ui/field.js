"use client";
// @flow
//
// Field: a label, a text input or text area, help text and an error, wired to
// each other and drawn as one form row.
//
// `uf ui add field` wrote this file into the project, and it is the project's
// from then on. `uf ui diff field` shows how it has moved away from the
// registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The label's type, the input's box and the help and error lines under it.
// `@uniflowed/ui`'s `Field` owns the wiring: the ids the label and the control
// share, one `aria-describedby` that names the description and the error only
// while each is there, `aria-invalid` only while the field is invalid,
// `aria-required`, and an error that is announced when it appears. The box's
// invalid look follows `aria-invalid` through `:is([aria-invalid=true])`.
//
// # What to keep true when you change it
//
// * **Every field has a label.** A placeholder is not one: it disappears as
//   soon as a reader types.
// * **Invalid is more than a colour.** The border turns `danger` and
//   `FieldError` says what is wrong in words, which is what a reader hears.
// * **Say how to fix it.** "Enter an address with a domain" rather than
//   "Invalid".
// * **Text stays on measured pairs.** `ink` on `surface`, `muted` on `surface`
//   and `sunken`, which `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1
//   in both themes. The error is `danger` on `surface`, about 6.6:1 in the
//   light default and 6.1:1 in the dark theme, a pair that suite does not
//   measure yet — check it if you change either colour.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import * as Primitive from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  root: {
    display: "grid",
    gap: ufTokens.space1,
    fontFamily: ufTokens.fontSans,
    color: ufTokens.ink,
  },
  label: {
    fontSize: ufTokens.textSm,
    fontWeight: ufTokens.weightMedium,
    lineHeight: ufTokens.leadingTight,
  },
  control: {
    boxSizing: "border-box",
    width: "100%",
    minHeight: "36px",
    margin: 0,
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space3,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingBase,
    color: { default: ufTokens.ink, ":disabled": ufTokens.muted },
    backgroundColor: { default: ufTokens.surface, ":disabled": ufTokens.sunken },
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: { default: ufTokens.border, ":is([aria-invalid=true])": ufTokens.danger },
    borderRadius: ufTokens.radiusSm,
    cursor: { default: "text", ":disabled": "not-allowed" },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "1px",
    "::placeholder": {
      color: ufTokens.muted,
      opacity: 1,
    },
  },
  textarea: {
    minHeight: "5rem",
    resize: "vertical",
  },
  description: {
    margin: 0,
    fontSize: ufTokens.textSm,
    color: ufTokens.muted,
  },
  status: {
    margin: 0,
    fontSize: ufTokens.textSm,
    color: ufTokens.muted,
  },
  error: {
    margin: 0,
    fontSize: ufTokens.textSm,
    fontWeight: ufTokens.weightMedium,
    color: ufTokens.danger,
  },
});

/**
 * The row. `invalid` and `required` reach the control; a field from
 * `@uniflowed/form` passes `field` instead.
 */
export component Field(
  children: React.Node,
  invalid?: boolean = false,
  required?: boolean = false,
  busy?: boolean = false,
  field?: Primitive.FieldSource,
  group?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.FieldRoot
      {...forwarded(rest)}
      busy={busy}
      className={classNames(props(styles.root, xstyle).className, className)}
      field={field}
      group={group}
      invalid={invalid}
      required={required}
    >
      {children}
    </Primitive.FieldRoot>
  );
}

/** The field's name. */
export component FieldLabel(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.FieldLabel
      {...forwarded(rest)}
      className={classNames(props(styles.label, xstyle).className, className)}
    >
      {children}
    </Primitive.FieldLabel>
  );
}

/** A text input, wired to the label, the description and the error. */
export component FieldInput(
  type?: string = "text",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.FieldControl
      render={(control) => (
        <input
          {...rest}
          {...forwarded(control)}
          className={classNames(props(styles.control, xstyle).className, className)}
          type={type}
        />
      )}
    />
  );
}

/** A text area, wired the same way as `FieldInput`. */
export component FieldTextarea(xstyle?: StyleArgument, className?: string, ...rest: Rest) {
  return (
    <Primitive.FieldControl
      render={(control) => (
        <textarea
          {...rest}
          {...forwarded(control)}
          className={classNames(
            props(styles.control, styles.textarea, xstyle).className,
            className,
          )}
        />
      )}
    />
  );
}

/** Help that stays under the field, and is read with it. */
export component FieldDescription(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.FieldDescription
      {...forwarded(rest)}
      className={classNames(props(styles.description, xstyle).className, className)}
    >
      {children}
    </Primitive.FieldDescription>
  );
}

/** Progress while the field is being saved or checked, announced politely. */
export component FieldStatus(
  children?: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.FieldStatus
      {...forwarded(rest)}
      className={classNames(props(styles.status, xstyle).className, className)}
    >
      {children}
    </Primitive.FieldStatus>
  );
}

/** What is wrong and how to fix it, shown only while the field is invalid. */
export component FieldError(
  children?: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.FieldError
      {...forwarded(rest)}
      className={classNames(props(styles.error, xstyle).className, className)}
    >
      {children}
    </Primitive.FieldError>
  );
}

/**
 * A caller's props on their way into a part rather than onto an element. Flow
 * checks that spread against the part's own `...rest`, whose `key` is `empty`
 * where this file's indexer says `mixed`; `@uniflowed/ui` papers over the same
 * hole the same way, and nothing checked is lost, because the elements its
 * parts render have `any`-typed props in uf's library today.
 */
function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}

/** The classes this file chose, then the caller's. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
