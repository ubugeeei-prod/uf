// @flow
import type { PlainDate } from "@uniflowed/core/temporal";
import { Temporal } from "@uniflowed/core/temporal";
export type DateRange = {| readonly start: string, readonly end: string |};
export function validateRange(range: DateRange | null): void {
  if (range == null) return;
  const start = Temporal.PlainDate.from(range.start).toString();
  const end = Temporal.PlainDate.from(range.end).toString();
  if (start > end) throw new RangeError("A date range must start on or before its end");
}

export function unavailableInRange(
  start: string,
  end: string,
  unavailable: ((date: PlainDate) => boolean) | void,
): boolean {
  if (unavailable == null) return false;
  for (
    let date = Temporal.PlainDate.from(start);
    date.toString() <= end;
    date = date.add({ days: 1 })
  ) {
    if (unavailable(date)) return true;
  }
  return false;
}
