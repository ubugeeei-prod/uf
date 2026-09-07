// @flow
//
// Temporal, the clock behind it, and the seeded stream beside it.
//
// The claim these three modules make is one sentence: two renders of the same
// page, on two machines, in two zones, produce the same bytes. Nothing here
// renders anything — that is `web.test.js` and `hooks.test.js` — and what is
// checked instead is the arithmetic underneath, one property at a time, because
// a hydration mismatch reported from a component is a fact about a component
// and gives no clue which of the twenty operations below was wrong.
//
// # Why the zone tests name Tokyo and Kathmandu
//
// Tokyo is nine hours ahead with no daylight saving, so it is the case where
// the calendar date differs from UTC's and nothing else moves. Kathmandu is
// +05:45, which is the case that catches an offset computed in whole hours —
// and an offset computed in whole hours is right for most of the world and
// wrong for two hundred million people.
//
// # Why nothing here waits
//
// Every test that involves "now" installs a clock. That is the whole point of
// `@uniflowed/core/clock`: before it, a test of "3 minutes ago" either waited
// three minutes or reached into a global, and both are why timing tests are the
// ones people mark as flaky and stop reading.

import { afterEach, describe, expect, it } from "@uniflowed/test";
import type { Clock } from "@uniflowed/core/clock";
import {
  currentClock,
  fixedClock,
  hostTimeZone,
  manualClock,
  setClock,
  systemClock,
} from "@uniflowed/core/clock";
import { currentRandom, hostSeed, seededRandom, setRandom, shuffled } from "@uniflowed/core/random";
import { Temporal, isLite } from "@uniflowed/core/temporal";

/** Every `setClock` and `setRandom` a test installs, undone after it. */
const undo: Array<() => void> = [];

afterEach(() => {
  while (undo.length > 0) {
    const put = undo.pop();
    if (put != null) {
      put();
    }
  }
});

/** Install `clock` for one test. */
function install(clock: Clock) {
  undo.push(setClock(clock));
}

/** 2026-09-04T06:00:00Z, which is 15:00 in Tokyo and 11:45 in Kathmandu. */
const AFTERNOON = Date.UTC(2026, 8, 4, 6, 0, 0);

describe("the clock seam", () => {
  it("is the host's until something installs one", () => {
    expect(currentClock().now()).toBeGreaterThan(Date.UTC(2020, 0, 1));
    expect(typeof currentClock().timeZone()).toBe("string");
  });

  it("stops when a fixed clock is installed, so two reads agree", () => {
    // The property a server render depends on: two components asking what time
    // it is during one render must not get two answers, or "today" is one date
    // at the top of the page and another at the bottom.
    install(fixedClock(AFTERNOON, "UTC"));

    expect(currentClock().now()).toBe(AFTERNOON);
    expect(currentClock().now()).toBe(AFTERNOON);
    expect(currentClock().timeZone()).toBe("UTC");
  });

  it("puts back what it found rather than the host's, so nesting works", () => {
    // The bug this prevents: two installs, each restoring "the system clock"
    // instead of what it found, leave the second one's clock installed for
    // everything that runs after them.
    const outer = setClock(fixedClock(1000, "UTC"));
    const inner = setClock(fixedClock(2000, "UTC"));

    inner();
    expect(currentClock().now()).toBe(1000);
    outer();
    expect(currentClock().now()).not.toBe(1000);
  });

  it("moves exactly as far as a test moves it", () => {
    const clock = manualClock(AFTERNOON, "UTC");
    install(clock.clock);

    clock.advance(90_000);
    expect(currentClock().now()).toBe(AFTERNOON + 90_000);
    clock.set(0);
    expect(currentClock().now()).toBe(0);
  });

  it("names a zone even where the host cannot", () => {
    // `hostTimeZone` falls back to UTC rather than guessing from the offset: an
    // offset is not a zone, and a guessed name is confidently wrong twice a year.
    expect(hostTimeZone().length).toBeGreaterThan(0);
    expect(systemClock().timeZone()).toBe(hostTimeZone());
  });
});

describe("Temporal.Instant", () => {
  it("refuses a string that is a date rather than an instant", () => {
    // `Date.parse("2026-09-04")` answers with a UTC midnight, and browsers once
    // answered with a local one. Temporal refuses the question, so Lite refuses
    // it too — being more permissive here would mean code that works on this
    // host throws on one that has the real Temporal.
    expect(() => Temporal.Instant.from("2026-09-04")).toThrow();
    expect(() => Temporal.Instant.from("not a time")).toThrow();
    expect(Temporal.Instant.from("2026-09-04T06:00:00Z").epochMilliseconds).toBe(AFTERNOON);
    expect(Temporal.Instant.from("2026-09-04T15:00:00+09:00").epochMilliseconds).toBe(AFTERNOON);
  });

  it("prints no fraction when there is none, as native Temporal does", () => {
    // `Date.prototype.toISOString` always prints `.000`. A text difference
    // between the Lite implementation and the host's would only show up on a
    // browser that had shipped Temporal, which is the worst place to find one.
    expect(Temporal.Instant.fromEpochMilliseconds(AFTERNOON).toString()).toBe(
      "2026-09-04T06:00:00Z",
    );
    expect(Temporal.Instant.fromEpochMilliseconds(AFTERNOON + 250).toString()).toBe(
      "2026-09-04T06:00:00.250Z",
    );
  });

  it("compares through compare, and refuses to be compared with <", () => {
    const early = Temporal.Instant.fromEpochMilliseconds(AFTERNOON);
    const late = Temporal.Instant.fromEpochMilliseconds(AFTERNOON + 1);

    expect(Temporal.Instant.compare(early, late)).toBe(-1);
    expect(Temporal.Instant.compare(late, early)).toBe(1);
    expect(Temporal.Instant.compare(early, early)).toBe(0);
    expect(early.equals(late)).toBe(false);

    if (isLite) {
      // `a < b` would coerce both to text and compare the strings, which is
      // right until one of them crosses a digit.
      expect(() => Number(early)).toThrow();
    }
  });

  it("adds what has a fixed length and refuses what does not", () => {
    const at = Temporal.Instant.fromEpochMilliseconds(AFTERNOON);

    expect(at.add({ hours: 2 }).toString()).toBe("2026-09-04T08:00:00Z");
    expect(at.subtract("PT30M").toString()).toBe("2026-09-04T05:30:00Z");
    // A month is 28, 29, 30 or 31 days depending on which month, so an instant
    // — which has no calendar — cannot add one.
    expect(() => at.add({ months: 1 })).toThrow();
  });

  it("measures the gap to another instant", () => {
    const at = Temporal.Instant.fromEpochMilliseconds(AFTERNOON);
    const later = Temporal.Instant.fromEpochMilliseconds(AFTERNOON + 185_500);

    expect(at.until(later).total({ unit: "minute" })).toBeCloseTo(3.0917, 3);
    expect(later.since(at).total({ unit: "second" })).toBe(185.5);
    expect(at.since(later).total({ unit: "second" })).toBe(-185.5);
  });
});

describe("Temporal.ZonedDateTime", () => {
  it("shows the wall clock of the zone it was asked for", () => {
    const zoned =
      Temporal.Instant.fromEpochMilliseconds(AFTERNOON).toZonedDateTimeISO("Asia/Tokyo");

    expect(zoned.year).toBe(2026);
    expect(zoned.month).toBe(9);
    expect(zoned.day).toBe(4);
    expect(zoned.hour).toBe(15);
    expect(zoned.minute).toBe(0);
    expect(zoned.offset).toBe("+09:00");
    expect(zoned.timeZoneId).toBe("Asia/Tokyo");
  });

  it("keeps the minutes of an offset that is not a whole hour", () => {
    // The case an offset computed in hours gets wrong, and the reason this test
    // names Kathmandu rather than a second European city.
    const zoned =
      Temporal.Instant.fromEpochMilliseconds(AFTERNOON).toZonedDateTimeISO("Asia/Kathmandu");

    expect(zoned.offset).toBe("+05:45");
    expect(zoned.hour).toBe(11);
    expect(zoned.minute).toBe(45);
  });

  it("reports the zone's own calendar date, not UTC's", () => {
    // 22:00 UTC is the following morning in Tokyo. A component that took the
    // date from the instant would print yesterday for every reader east of it.
    const evening = Date.UTC(2026, 8, 4, 22, 0, 0);
    const zoned = Temporal.Instant.fromEpochMilliseconds(evening).toZonedDateTimeISO("Asia/Tokyo");

    expect(zoned.toPlainDate().toString()).toBe("2026-09-05");
    expect(zoned.dayOfWeek).toBe(6);
    expect(Temporal.Instant.fromEpochMilliseconds(evening).toString().slice(0, 10)).toBe(
      "2026-09-04",
    );
  });

  it("survives a round trip through its own text", () => {
    const zoned =
      Temporal.Instant.fromEpochMilliseconds(AFTERNOON).toZonedDateTimeISO("Asia/Tokyo");

    expect(zoned.toString()).toBe("2026-09-04T15:00:00+09:00[Asia/Tokyo]");
    expect(Temporal.ZonedDateTime.from(zoned.toString()).equals(zoned)).toBe(true);
  });

  it("formats in its own zone rather than in the machine's", () => {
    // The difference from `Date.prototype.toLocaleString`, and the reason this
    // component can be prerendered: the zone is a property of the value, so a
    // caller who does not name one still does not get the host's.
    const zoned =
      Temporal.Instant.fromEpochMilliseconds(AFTERNOON).toZonedDateTimeISO("Asia/Tokyo");

    expect(
      zoned.toLocaleString("en-US", { hour: "2-digit", minute: "2-digit", hour12: false }),
    ).toBe("15:00");
    expect(
      zoned.toLocaleString("en-US", {
        timeZone: "UTC",
        hour: "2-digit",
        minute: "2-digit",
        hour12: false,
      }),
    ).toBe("06:00");
  });

  it("moves the offset across a daylight-saving boundary", () => {
    // New York is -04:00 in August and -05:00 in December. An implementation
    // that read the offset once and kept it would be an hour wrong for four
    // months of every year.
    const summer =
      Temporal.Instant.from("2026-08-01T12:00:00Z").toZonedDateTimeISO("America/New_York");
    const winter =
      Temporal.Instant.from("2026-12-01T12:00:00Z").toZonedDateTimeISO("America/New_York");

    expect(summer.offset).toBe("-04:00");
    expect(winter.offset).toBe("-05:00");
  });
});

describe("Temporal.PlainDate", () => {
  it("is a date with no time and no zone to lose it to", () => {
    const date = Temporal.PlainDate.from("2026-09-04");

    expect(date.year).toBe(2026);
    expect(date.month).toBe(9);
    expect(date.day).toBe(4);
    expect(date.toString()).toBe("2026-09-04");
  });

  it("clamps to the end of the month rather than rolling into the next", () => {
    // The 31st of January plus one month is the 28th of February. Rolling over
    // is how a subscription billed "monthly on the 31st" quietly moves to March
    // and then never bills in February again.
    expect(Temporal.PlainDate.from("2026-01-31").add({ months: 1 }).toString()).toBe("2026-02-28");
    expect(Temporal.PlainDate.from("2024-01-31").add({ months: 1 }).toString()).toBe("2024-02-29");
    expect(Temporal.PlainDate.from("2026-09-04").add({ days: 30 }).toString()).toBe("2026-10-04");
    expect(Temporal.PlainDate.from("2026-01-15").subtract({ months: 2 }).toString()).toBe(
      "2025-11-15",
    );
  });

  it("orders two dates", () => {
    const early = Temporal.PlainDate.from("2026-09-04");
    const late = Temporal.PlainDate.from("2026-09-05");

    expect(Temporal.PlainDate.compare(early, late)).toBe(-1);
    expect(Temporal.PlainDate.compare(late, late)).toBe(0);
    expect(early.equals(late)).toBe(false);
  });
});

describe("Temporal.Duration", () => {
  it("stays in the units it was written in", () => {
    // `PT90M` is ninety minutes and prints as ninety minutes. Balancing it into
    // an hour and a half would print something the caller did not write.
    expect(Temporal.Duration.from("PT90M").toString()).toBe("PT90M");
    expect(Temporal.Duration.from({ days: 1, hours: 2 }).toString()).toBe("P1DT2H");
    expect(Temporal.Duration.from({}).toString()).toBe("PT0S");
    expect(Temporal.Duration.from({}).blank).toBe(true);
  });

  it("totals what has a fixed length and refuses what does not", () => {
    expect(Temporal.Duration.from("PT90M").total({ unit: "hour" })).toBe(1.5);
    expect(Temporal.Duration.from("P1D").total({ unit: "hours" })).toBe(24);
    expect(Temporal.Duration.from("PT0.25S").total({ unit: "millisecond" })).toBe(250);
    // A month has no length until you say which month, and Temporal refuses it
    // here for the same reason rather than averaging one.
    expect(() => Temporal.Duration.from({ months: 1 }).total({ unit: "day" })).toThrow();
  });

  it("negates every field, so a negative duration reads as one", () => {
    expect(Temporal.Duration.from("PT30M").negated().toString()).toBe("-PT30M");
    expect(Temporal.Duration.from("-PT30M").abs().toString()).toBe("PT30M");
    expect(Temporal.Duration.from("-PT30M").total({ unit: "minute" })).toBe(-30);
  });

  it("refuses text that is not a duration", () => {
    expect(() => Temporal.Duration.from("30 minutes")).toThrow();
    expect(() => Temporal.Duration.from("P")).toThrow();
  });
});

describe("Temporal.Now", () => {
  it("reads the injected clock rather than the host's", () => {
    // The single most important line in this file. `Temporal.Now.instant()` in a
    // render is the bug the whole area exists to fix, and it is only fixable if
    // `Now` is uf's even on a host that has Temporal of its own.
    install(fixedClock(AFTERNOON, "Asia/Tokyo"));

    expect(Temporal.Now.instant().epochMilliseconds).toBe(AFTERNOON);
    expect(Temporal.Now.timeZoneId()).toBe("Asia/Tokyo");
    expect(Temporal.Now.zonedDateTimeISO().hour).toBe(15);
    expect(Temporal.Now.plainDateISO().toString()).toBe("2026-09-04");
    expect(Temporal.Now.plainTimeISO().toString()).toBe("15:00:00");
  });

  it("takes a zone that is not the clock's", () => {
    install(fixedClock(AFTERNOON, "UTC"));

    expect(Temporal.Now.zonedDateTimeISO("Asia/Tokyo").hour).toBe(15);
    expect(Temporal.Now.plainTimeISO("UTC").toString()).toBe("06:00:00");
  });
});

describe("the seeded stream", () => {
  it("replays the same numbers from the same seed", () => {
    // What makes the server's shuffle and the browser's the same shuffle.
    const first = seededRandom("uf-509");
    const second = seededRandom("uf-509");

    const drawn = [first.next(), first.next(), first.next()];
    expect(drawn).toEqual([second.next(), second.next(), second.next()]);
    for (const value of drawn) {
      expect(value).toBeGreaterThanOrEqual(0);
      expect(value).toBeLessThan(1);
    }
  });

  it("gives adjacent seeds unrelated streams", () => {
    // Seeds in practice are adjacent — a counter, a request id — and a weak
    // hash would give them streams that begin with visibly similar numbers,
    // which is a shuffle that is not a shuffle.
    expect(seededRandom("uf-1").next()).not.toBeCloseTo(seededRandom("uf-2").next(), 2);
    expect(seededRandom(7).next()).toBe(seededRandom("7").next());
  });

  it("forks by name, so the draw order does not matter", () => {
    // The bug this prevents is the nastiest one in the area: with a single
    // shared stream, a boundary that suspends on the server and not in the
    // browser reorders every draw after it, and the page hydrates wrong only on
    // a slow connection.
    const parent = seededRandom("page");
    const sidebarFirst = parent.fork("sidebar").next();
    parent.next();
    parent.next();

    expect(seededRandom("page").fork("sidebar").next()).toBe(sidebarFirst);
    expect(parent.fork("featured").next()).not.toBe(sidebarFirst);
  });

  it("shuffles the same way on both sides and leaves the input alone", () => {
    const items = ["a", "b", "c", "d", "e", "f"];
    const server = shuffled(items, seededRandom("page").fork("featured"));
    const client = shuffled(items, seededRandom("page").fork("featured"));

    expect(server).toEqual(client);
    expect(server.slice().sort()).toEqual(items);
    // A shuffle that mutated its argument would be changing a prop, and React
    // compares props.
    expect(items).toEqual(["a", "b", "c", "d", "e", "f"]);
  });

  it("draws integers inside the bound", () => {
    const random = seededRandom("bounds");
    for (let at = 0; at < 200; at += 1) {
      const drawn = random.integer(5);
      expect(drawn).toBeGreaterThanOrEqual(0);
      expect(drawn).toBeLessThan(5);
    }
    expect(random.integer(0)).toBe(0);
    expect(random.integer(-1)).toBe(0);
  });

  it("is the process's stream until something installs one", () => {
    const put = setRandom(seededRandom("installed"));
    undo.push(put);

    expect(currentRandom().next()).toBe(seededRandom("installed").next());
    put();
    undo.pop();
    expect(typeof currentRandom().next()).toBe("number");
  });

  it("asks the host for a seed only where there is nothing to agree with", () => {
    // Eight base-36 characters. Short enough to sit in the markup unnoticed, and
    // wide enough that two pages rendered in the same millisecond do not collide.
    const seed = hostSeed();

    expect(seed.length).toBe(8);
    expect(/^[0-9a-z]{8}$/.test(seed)).toBe(true);
    expect(hostSeed()).not.toBe(seed);
  });
});
