// @flow

/** Local, attributed stock media and its display metadata; no remote user upload is implied. */
export type Clip = {|
  readonly id: string,
  readonly title: string,
  readonly description: string,
  readonly src: string,
  readonly poster: string,
  readonly credit: string,
  readonly source: string,
|};

/**
 * Explicit browser media states, including recoverable autoplay rejection and loading failure.
 */
export type Playback =
  | {| kind: "loading" |}
  | {| kind: "playing" |}
  | {| kind: "paused" |}
  | {| kind: "blocked" |}
  | {| kind: "error" |};
// Curated public media; no user data or server capability is needed to browse it.

/** Curated local fixtures whose sources and licenses are recorded in public/media/CREDITS.md. */
export const CLIPS: $ReadOnlyArray<Clip> = [
  {
    id: "after-dark",
    title: "After dark",
    description: "A different rhythm above the city.",
    src: "/media/clips/city.mp4",
    poster: "/media/clips/city.jpg",
    credit: "Rubén Velasco",
    source: "https://mixkit.co/free-stock-video/big-city-at-night-from-an-aerial-shot-49878/",
  },
  {
    id: "last-train",
    title: "The last light",
    description: "Watching the evening settle over the tracks.",
    src: "/media/clips/rail.mp4",
    poster: "/media/clips/rail.jpg",
    credit: "Rubén Velasco",
    source: "https://mixkit.co/free-stock-video/sunset-over-train-tracks-1594/",
  },
  {
    id: "long-way",
    title: "The long way home",
    description: "Twelve seconds away from the desk.",
    src: "/media/clips/park.mp4",
    poster: "/media/clips/park.jpg",
    credit: "Edgar Fernandez",
    source: "https://mixkit.co/free-stock-video/slowly-walking-down-a-path-in-a-park-40656/",
  },
];
