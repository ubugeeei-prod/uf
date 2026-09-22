// @flow
import * as React from "@uniflowed/react";
import { afterEach, expect, fn, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent, within } from "@uniflowed/react-testing";
import { DateField, TimeField, RangeCalendar, DateRangePicker, I18nProvider } from "./index.js";

afterEach(cleanup);
it("edits locale-ordered date segments and constrains leap days with the keyboard", async () => {
  const change = fn();
  render(<DateField aria-label="Birthday" defaultValue="2024-02-29" onValueChange={change} />);
  const year = screen.getByRole("spinbutton", { name: "year" });
  year.focus();
  await userEvent.keyboard("{ArrowUp}{Enter}");
  expect(change).toHaveBeenLastCalledWith("2025-02-28");
  expect(screen.getByRole("spinbutton", { name: "day" }).value).toBe("28");
});
it("rejects incomplete and out-of-range dates without changing the value", async () => {
  const change = fn();
  render(
    <DateField
      aria-label="Start"
      defaultValue="2026-02-20"
      minValue="2026-02-10"
      onValueChange={change}
    />,
  );
  const day = screen.getByRole("spinbutton", { name: "day" });
  await userEvent.clear(day);
  await userEvent.type(day, "1");
  await userEvent.keyboard("{Enter}");
  expect(screen.getByRole("group").getAttribute("aria-invalid")).toBe("true");
  expect(change).not.toHaveBeenCalled();
  expect(screen.getByRole("status").textContent).toContain("valid value");
});
it("uses Arabic digits and mirrored segment navigation", async () => {
  render(
    <I18nProvider locale="ar-EG">
      <DateField aria-label="Date" defaultValue="2026-02-20" />
    </I18nProvider>,
  );
  const segments = screen.getAllByRole("spinbutton");
  expect(segments[0].value).toBe("٢٠");
  segments[0].focus();
  await userEvent.keyboard("{ArrowLeft}");
  expect(document.activeElement).toBe(segments[1]);
});
it("edits and announces time segments", async () => {
  const change = fn();
  render(
    <TimeField
      aria-label="Alarm"
      defaultValue="12:59:00"
      granularity="second"
      onValueChange={change}
    />,
  );
  const minute = screen.getByRole("spinbutton", { name: "minute" });
  minute.focus();
  await userEvent.keyboard("{ArrowUp}{Enter}");
  expect(change).toHaveBeenLastCalledWith("12:00:00");
});
it("chooses an inclusive range by keyboard and refuses unavailable dates inside it", async () => {
  const change = fn();
  const { rerender } = render(
    <RangeCalendar.Root defaultFocused="2026-02-10" today="2026-02-10" onValueChange={change} />,
  );
  const start = screen.getByRole("gridcell", { name: "10" });
  start.focus();
  await userEvent.keyboard("{Enter}");
  await userEvent.keyboard("{ArrowRight}");
  await userEvent.keyboard("{ArrowRight}");
  await userEvent.keyboard("{Enter}");
  expect(change).toHaveBeenLastCalledWith({ start: "2026-02-10", end: "2026-02-12" });
  expect(
    screen.getAllByRole("gridcell").filter((cell) => cell.getAttribute("aria-selected") === "true")
      .length,
  ).toBe(3);
  change.mockClear();
  rerender(
    <RangeCalendar.Root
      defaultFocused="2026-02-10"
      today="2026-02-10"
      onValueChange={change}
      isDateDisabled={(date) => date.day === 11}
    />,
  );
  screen.getByRole("gridcell", { name: "10" }).focus();
  await userEvent.keyboard("{Enter}");
  await userEvent.keyboard("{ArrowRight}");
  await userEvent.keyboard("{ArrowRight}");
  await userEvent.keyboard("{Enter}");
  expect(change).not.toHaveBeenCalled();
  expect(screen.getByText("The range contains an unavailable date")).not.toBe(null);
});
it("opens a date range picker, closes after selection, and passes axe", async () => {
  const { container } = render(
    <main>
      <h1>Stay</h1>
      <DateRangePicker.Root defaultValue={{ start: "2026-02-10", end: "2026-02-12" }}>
        <DateRangePicker.StartField aria-label="Arrival" />
        <DateRangePicker.EndField aria-label="Departure" />
        <DateRangePicker.Trigger>Choose dates</DateRangePicker.Trigger>
        <DateRangePicker.Calendar aria-label="Choose a stay" />
      </DateRangePicker.Root>
      <TimeField aria-label="Check in" defaultValue="12:30" />
    </main>,
  );
  await userEvent.click(screen.getByRole("button", { name: "Choose dates" }));
  const grid = screen.getByRole("grid");
  await userEvent.click(within(grid).getByRole("gridcell", { name: "15" }));
  await userEvent.click(within(grid).getByRole("gridcell", { name: "18" }));
  expect(screen.queryByRole("grid")).toBe(null);
  expect(document.activeElement).toBe(screen.getByRole("button", { name: "Choose dates" }));
  await expect(container).toHaveNoAxeViolations();
});

it("edits the locale day period while storing a 24-hour ISO time", async () => {
  const change = fn();
  render(<TimeField aria-label="Meeting" defaultValue="11:30" onValueChange={change} />);
  const period = screen.getByRole("spinbutton", { name: "AM/PM" });
  period.focus();
  await userEvent.keyboard("{ArrowUp}");
  await userEvent.keyboard("{Enter}");
  expect(change).toHaveBeenLastCalledWith("23:30");
  const hour = screen.getByRole("spinbutton", { name: "hour" });
  hour.focus();
  await userEvent.keyboard("{End}");
  await userEvent.keyboard("{Enter}");
  expect(change).toHaveBeenLastCalledWith("12:30");
});
it("compares time bounds at the selected granularity", async () => {
  const change = fn();
  render(
    <TimeField
      aria-label="Boundary"
      defaultValue="12:30"
      minValue="12:30:00"
      hourCycle="h23"
      onValueChange={change}
    />,
  );
  screen.getByRole("spinbutton", { name: "minute" }).focus();
  await userEvent.keyboard("{Enter}");
  expect(change).toHaveBeenLastCalledWith("12:30");
});

it("rejects typed ranges that cross an unavailable day", async () => {
  const change = fn();
  render(
    <DateRangePicker.Root
      defaultValue={{ start: "2026-02-10", end: "2026-02-10" }}
      isDateDisabled={(date) => date.day === 11}
      onValueChange={change}
    >
      <DateRangePicker.EndField aria-label="Departure" />
    </DateRangePicker.Root>,
  );
  const day = screen.getByRole("spinbutton", { name: "day" });
  await userEvent.clear(day);
  await userEvent.type(day, "12");
  await userEvent.keyboard("{Enter}");
  expect(change).not.toHaveBeenCalled();
  expect(screen.getByRole("group").getAttribute("aria-invalid")).toBe("true");
});
