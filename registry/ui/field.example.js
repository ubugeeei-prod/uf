// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import {
  Field,
  FieldDescription,
  FieldError,
  FieldInput,
  FieldLabel,
  FieldTextarea,
} from "./field.js";

const styles = stylex.create({
  form: {
    display: "grid",
    gap: ufTokens.space4,
    maxWidth: "24rem",
  },
});

/** A field with help, one that is invalid and says why, and a text area. */
export component Example() {
  return (
    <form {...props(styles.form)}>
      <Field required>
        <FieldLabel>Name</FieldLabel>
        <FieldInput autoComplete="name" defaultValue="Ada Lovelace" name="name" />
        <FieldDescription>As it should appear on receipts.</FieldDescription>
      </Field>
      <Field invalid required>
        <FieldLabel>Email</FieldLabel>
        <FieldInput autoComplete="email" defaultValue="ada@example" name="email" type="email" />
        <FieldError>Enter an address with a domain, like ada@example.com.</FieldError>
      </Field>
      <Field>
        <FieldLabel>Note</FieldLabel>
        <FieldTextarea name="note" placeholder="Anything we should know" />
      </Field>
    </form>
  );
}
