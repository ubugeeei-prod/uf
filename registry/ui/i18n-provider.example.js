// @flow
import * as React from "@uniflowed/react";
import { I18nProvider } from "./i18n-provider.js";
import { NumberField, NumberFieldInput } from "./number-field.js";

/** A named, keyboard-operable example using the default presentation. */
export component Example() {
  return (
    <I18nProvider locale="ar-EG" aria-label="Arabic settings" role="region">
      <NumberField defaultValue={2}>
        <NumberFieldInput aria-label="Quantity" />
      </NumberField>
    </I18nProvider>
  );
}
