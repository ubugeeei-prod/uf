// @flow
//
// `@uniflowed/temporal`.

import type { NativeHandle } from "./internal/native-runtime.js";
import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/temporal";

export opaque type Instant = NativeHandle<"@uniflowed/core/temporal#Instant">;
export opaque type PlainDate =
  NativeHandle<"@uniflowed/core/temporal#PlainDate">;
export opaque type PlainTime =
  NativeHandle<"@uniflowed/core/temporal#PlainTime">;
export opaque type ZonedDateTime =
  NativeHandle<"@uniflowed/core/temporal#ZonedDateTime">;
export opaque type Duration = NativeHandle<"@uniflowed/core/temporal#Duration">;

export const Temporal: {
  +Instant: { +from: (value: string) => Instant },
  +PlainDate: { +from: (value: string) => PlainDate },
  +PlainTime: { +from: (value: string) => PlainTime },
  +ZonedDateTime: { +from: (value: string) => ZonedDateTime },
  +Duration: { +from: (value: string) => Duration },
} = {
  Instant: {
    from: (value: string): Instant =>
      nativeRuntimeRequired(MODULE, "Temporal.Instant.from"),
  },
  PlainDate: {
    from: (value: string): PlainDate =>
      nativeRuntimeRequired(MODULE, "Temporal.PlainDate.from"),
  },
  PlainTime: {
    from: (value: string): PlainTime =>
      nativeRuntimeRequired(MODULE, "Temporal.PlainTime.from"),
  },
  ZonedDateTime: {
    from: (value: string): ZonedDateTime =>
      nativeRuntimeRequired(MODULE, "Temporal.ZonedDateTime.from"),
  },
  Duration: {
    from: (value: string): Duration =>
      nativeRuntimeRequired(MODULE, "Temporal.Duration.from"),
  },
};
