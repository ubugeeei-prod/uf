// @flow
import * as React from "@uniflowed/react";
import { afterEach, describe, expect, fn, it } from "@uniflowed/test";
import { cleanup, fireEvent, render, screen, userEvent } from "@uniflowed/react-testing";
import {
  Field,
  I18nProvider,
  NumberField,
  Tabs,
  Select,
  useLocale,
  useFilter,
  useCollator,
} from "./index.js";
import { parseNumber } from "./number-field.js";

afterEach(cleanup);

component Probe() {
  const { locale, direction } = useLocale();
  const filter = useFilter();
  const collator = useCollator({ sensitivity: "base" });
  return (
    <output>{`${locale} ${direction} ${String(filter.startsWith("İzmir", "i"))} ${collator.compare("İ", "i")}`}</output>
  );
}

describe("locale propagation", () => {
  it("propagates RTL and restores nested language islands", () => {
    render(
      <I18nProvider locale="ar">
        <Probe />
        <I18nProvider locale="tr">
          <Probe />
        </I18nProvider>
      </I18nProvider>,
    );
    expect(screen.getByText("ar rtl true 0").closest("[dir]")?.getAttribute("dir")).toBe("rtl");
    expect(screen.getByText("tr ltr true 0").closest("[lang]")?.getAttribute("lang")).toBe("tr");
  });

  it("mirrors horizontal arrows under an Arabic provider", async () => {
    render(
      <I18nProvider locale="ar">
        <Tabs.Root defaultValue="a">
          <Tabs.List aria-label="Sections">
            <Tabs.Tab value="a">A</Tabs.Tab>
            <Tabs.Tab value="b">B</Tabs.Tab>
          </Tabs.List>
          <Tabs.Panel value="a">One</Tabs.Panel>
          <Tabs.Panel value="b">Two</Tabs.Panel>
        </Tabs.Root>
      </I18nProvider>,
    );
    screen.getByRole("tab", { name: "A" }).focus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(document.activeElement).toBe(screen.getByRole("tab", { name: "B" }));
  });

  it("uses locale collation in select typeahead", async () => {
    render(
      <I18nProvider locale="tr">
        <Select.Root>
          <Select.Trigger aria-label="City">
            <Select.Value />
          </Select.Trigger>
          <Select.List>
            <Select.Option value="ankara">Ankara</Select.Option>
            <Select.Option value="izmir">İzmir</Select.Option>
          </Select.List>
        </Select.Root>
      </I18nProvider>,
    );
    screen.getByRole("combobox").focus();
    await userEvent.keyboard("i");
    expect(screen.getByRole("option", { name: "İzmir" }).id).toBe(
      screen.getByRole("combobox").getAttribute("aria-activedescendant"),
    );
  });
});

describe("NumberField", () => {
  it("parses currency, percent, units and Arabic digits without accepting junk", () => {
    const cases: $ReadOnlyArray<[string, Intl$NumberFormatOptions, number]> = [
      ["de-DE", { style: "currency", currency: "EUR" }, -1234.5],
      ["ar-EG", { style: "decimal" }, 1234.5],
      ["en-US", { style: "percent" }, 0.25],
      ["fr-FR", { style: "unit", unit: "kilogram" }, 12.5],
      ["en-US", { style: "currency", currency: "USD", currencySign: "accounting" }, -12.5],
    ];
    for (const [locale, options, value] of cases) {
      const format: $FlowFixMe = new Intl.NumberFormat(locale, options);
      expect(parseNumber(format.format(value), format)).toBe(value);
      expect(Number.isNaN(parseNumber("abc12", format))).toBe(true);
    }
  });

  it("steps with keys and pointer buttons, respects bounds, and wires Field ids", async () => {
    const changed = fn();
    const { container } = render(
      <main>
        <h1>Order</h1>
        <I18nProvider locale="de-DE">
          <Field.Root>
            <Field.Label>Weight</Field.Label>
            <NumberField.Root defaultValue={1} min={0} max={2} step={0.5} onValueChange={changed}>
              <Field.Control render={(props) => <NumberField.Input {...props} />} />
              <NumberField.Decrement />
              <NumberField.Increment />
            </NumberField.Root>
            <Field.Description>In kilograms</Field.Description>
          </Field.Root>
        </I18nProvider>
      </main>,
    );
    const input = screen.getByRole("spinbutton", { name: "Weight" });
    input.focus();
    await userEvent.keyboard("{ArrowUp}");
    expect(input).toHaveValue("1,5");
    await userEvent.click(screen.getByRole("button", { name: "Increase" }));
    expect(input).toHaveValue("2");
    expect(screen.getByRole("button", { name: "Increase" })).toBeDisabled();
    expect(input.getAttribute("aria-describedby")).toBe(screen.getByText("In kilograms").id);
    expect(changed).toHaveBeenLastCalledWith(2);
    await expect(container).toHaveNoAxeViolations();
  });

  it("keeps invalid text for correction and commits only valid stepped values", async () => {
    const changed = fn();
    render(
      <NumberField.Root defaultValue={2} min={0} max={10} step={2} onValueChange={changed}>
        <NumberField.Input aria-label="Count" />
      </NumberField.Root>,
    );
    const input = screen.getByRole("spinbutton");
    await userEvent.clear(input);
    await userEvent.type(input, "3");
    await userEvent.keyboard("{Enter}");
    expect(input.getAttribute("aria-invalid")).toBe("true");
    expect(changed).not.toHaveBeenCalled();
    await userEvent.clear(input);
    await userEvent.click(input);
    await userEvent.type(input, "4");
    await userEvent.keyboard("{Enter}");
    expect(changed).toHaveBeenLastCalledWith(4);
    expect(input.getAttribute("aria-invalid")).toBe(null);
  });
});

it("stops stepping at the last valid value below an unaligned maximum", async () => {
  render(
    <NumberField.Root defaultValue={0.6} min={0} max={0.9} step={0.2}>
      <NumberField.Input aria-label="Fraction" />
      <NumberField.Increment />
    </NumberField.Root>,
  );
  const button = screen.getByRole("button", { name: "Increase" });
  await userEvent.click(button);
  expect(screen.getByRole("spinbutton")).toHaveValue("0.8");
  expect(button).toBeDisabled();
});
