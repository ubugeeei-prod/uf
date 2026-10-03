"use client";
// @flow

import * as React from "@uniflowed/react";
import { useEffect, useRef, useState } from "@uniflowed/react";
import { promise, runPromiseExit } from "@uniflowed/effect";
import { props, stylex } from "@uniflowed/stylex";

import { Icon } from "../_shared/ui.js";

import type { Clip, Playback } from "./clip-model.js";

/** Convert a browser's rejected play request into a recoverable playback state. */

async function requestPlayback(player: HTMLVideoElement, blocked: () => void): Promise<void> {
  const result = await runPromiseExit(promise(() => player.play()));

  if (result.kind === "failure") {
    blocked();
  }
}

/**
 * Attach media only to the selected slide and synchronize its browser lifetime.
 * Autoplay respects reduced motion; backgrounding and cleanup pause playback.
 * A late rejection from an inactive slide cannot overwrite the current state.
 */

export component ClipPlayer(clip: Clip, active: boolean, muted: boolean, onMute: () => void) {
  const video                   = useRef<HTMLVideoElement | null>(null);
  const [playback, setPlayback] = useState<Playback>({ kind: "paused" });

  useEffect(() => {
    const player = video.current;
    if (player == null || !active) return;
    const lifetime = new AbortController();
    const play = (): void => {
      void requestPlayback(player, () => {
        if (!lifetime.signal.aborted) setPlayback({ kind: "blocked" });
      });
    };
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (!reduced && !document.hidden) play();
    const visibility = (): void => {
      if (document.hidden) player.pause();
    };
    document.addEventListener("visibilitychange", visibility);
    return () => {
      lifetime.abort();
      player.pause();
      document.removeEventListener("visibilitychange", visibility);
    };
  }, [active]);

  function toggle(): void {
    const player = video.current;
    if (player == null) return;
    if (player.paused) {
      void requestPlayback(player, () => {
        if (video.current === player && player.isConnected && player.hasAttribute("src")) {
          setPlayback({ kind: "blocked" });
        }
      });
    } else {
      player.pause();
    }
  }
  const playing = playback.kind === "playing";

  return (
    <div {...props(styles.clipStage)}>
      <video
        {...props(styles.clipVideo)}
        ref={video}
        src={
          match (active) {
            true  => clip.src,
            false => undefined,
          }
        }
        poster={clip.poster}
        preload={
          match (active) {
            true  => "auto",
            false => "none",
          }
        }
        muted={muted}
        playsInline
        loop
        aria-label={`${clip.title}. ${clip.description} Silent stock footage.`}
        onPlaying={() => setPlayback({ kind: "playing" })}
        onLoadedData={(event) => {
          if (event.currentTarget.paused) setPlayback({ kind: "paused" });
        }}
        onPause={() => setPlayback({ kind: "paused" })}
        onWaiting={() => setPlayback({ kind: "loading" })}
        onError={() => setPlayback({ kind: "error" })}
      />
      {
        match (active) {
          false => null,
          true  =>
            <>
              <div {...props(styles.clipControls)}>
                <button
                  {...props(styles.clipControl)}
                  type="button"
                  onClick={toggle}
                  aria-label={
                    match (playing) {
                      true  => "Pause video",
                      false => "Play video",
                    }
                  }
                >
                  <Icon
                    name={
                      match (playing) {
                        true  => "pause",
                        false => "play",
                      }
                    }
                    size={19}
                  />
                </button>
                <button
                  {...props(styles.clipControl)}
                  type="button"
                  onClick={onMute}
                  aria-label={
                    match (muted) {
                      true  => "Unmute video",
                      false => "Mute video",
                    }
                  }
                  aria-pressed={!muted}
                >
                  <Icon
                    name={
                      match (muted) {
                        true  => "muted",
                        false => "volume",
                      }
                    }
                    size={19}
                  />
                </button>
              </div>
              {
                match (playback) {
                  {kind: "error"}                      =>
                    <div {...props(styles.clipNotice)} role="alert">
                      Could not load this clip.{" "}
                      <button
                        {...props(styles.clipNoticeButton)}
                        type="button"
                        onClick={() => {
                          video.current?.load();
                          toggle();
                        }}
                      >
                        Try again
                      </button>
                    </div>,
                  {kind: "blocked"}                    =>
                    <button type="button" {...props(styles.clipPlay)} onClick={toggle}>
                      <Icon name="play" size={26} />
                      <span>Play video</span>
                    </button>,
                  {kind: "loading"}                    =>
                    <span {...props(styles.clipLoading)} role="status">
                      Loading video…
                    </span>,
                  {kind: "paused"} | {kind: "playing"} => null,
                }
              }
            </>,
        }
      }
      <div {...props(styles.clipCaption)}>
        <h2 {...props(styles.clipTitle)}>{clip.title}</h2>
        <p {...props(styles.clipDescription)}>{clip.description}</p>
        <a {...props(styles.clipCredit)} href={clip.source} target="_blank" rel="noreferrer">
          Film by {clip.credit} <span {...props(styles.clipCreditMark)}>↗</span>
        </a>
      </div>
    </div>
  );
}

/**
 * Coordinate one visible video through scroll snapping, visibility observation, and keyboard controls.
 * The observer owns selection; navigation scrolls to a slide without preloading every video.
 */

export component Clips(clips: $ReadOnlyArray<Clip>) {
  const viewport                = useRef<HTMLDivElement | null>(null);
  const [selected, setSelected] = useState(0);
  const [muted,    setMuted]    = useState(true);

  useEffect(() => {
    const element = viewport.current;
    if (element == null) return;
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting && entry.intersectionRatio >= 0.65) {
            const index = Number(entry.target.getAttribute("data-index"));
            setSelected(index);
          }
        }
      },
      { root: element, threshold: [0.65] },
    );
    for (const child of element.children) observer.observe(child);
    return () => observer.disconnect();
  }, []);

  function move(index: number): void {
    const element = viewport.current;
    if (element == null || index < 0 || index >= clips.length) return;
    element.scrollTo({
      top     : index * element.clientHeight,
      behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth",
    });
  }

  return (
    <div {...props(styles.clipsLayout)}>
      <div {...props(styles.clipsPlayer)}>
        <header {...props(styles.clipsBar)}>
          <h1 {...props(styles.clipsTitle)}>Clips</h1>
          <span {...props(styles.clipsCount)} aria-live="polite">
            {String(selected + 1).padStart(2, "0")}
            <span {...props(styles.clipsCountTotal)}>
              {" "}
              / {String(clips.length).padStart(2, "0")}
            </span>
          </span>
        </header>
        <div
          ref={viewport}
          {...props(styles.clipsViewport)}
          tabIndex={0}
          role="region"
          aria-label="Clips. Scroll or use up and down arrows to change video."
          onKeyDown={(event) => {
            if (event.target !== event.currentTarget) return;
            if (event.key === "ArrowDown" || event.key === "ArrowUp") {
              event.preventDefault();
              move(selected + (event.key === "ArrowDown" ? 1 : -1));
            }
          }}
        >
          {clips.map((clip, index) => (
            <section
              {...props(styles.clipSlide)}
              key={clip.id}
              data-index={index}
              aria-label={`${index + 1} of ${clips.length}: ${clip.title}`}
            >
              <ClipPlayer
                clip={clip}
                active={index === selected}
                muted={muted}
                onMute={() => setMuted(!muted)}
              />
            </section>
          ))}
        </div>
      </div>
      <nav {...props(styles.clipsNavigation)} aria-label="Video navigation">
        <button
          {...props(styles.clipsNavButton)}
          type="button"
          onClick={() => move(selected - 1)}
          disabled={selected === 0}
          aria-label="Previous clip"
        >
          <Icon name="chevron-up" />
        </button>
        <button
          {...props(styles.clipsNavButton)}
          type="button"
          onClick={() => move(selected + 1)}
          disabled={selected === clips.length - 1}
          aria-label="Next clip"
        >
          <Icon name="chevron-down" />
        </button>
      </nav>
    </div>
  );
}

const styles = stylex.create({
  clipStage: {
    height  : "100%",
    width   : "100%",
    position: "relative",
    overflow: "hidden",
    color   : "white",
  },
  clipVideo: {
    width    : "100%",
    height   : "100%",
    objectFit: "cover",
    display  : "block",
  },
  clipControls: {
    position      : "absolute",
    inset         : "18px 18px auto",
    display       : "flex",
    justifyContent: "flex-end",
    gap           : "8px",
  },
  clipControl: {
    color         : "#fff",
    background    : "#17171780",
    border        : "1px solid #ffffff40",
    backdropFilter: "blur(16px)",
    display       : "grid",
    placeItems    : "center",
    width         : "42px",
    height        : "42px",
    borderRadius  : "50%",
  },
  clipNotice: {
    color         : "#fff",
    background    : "#17171780",
    border        : "1px solid #ffffff40",
    backdropFilter: "blur(16px)",
    position      : "absolute",
    inset         : "35% 20px auto",
    fontSize      : "13px",
    paddingTop    : "20px",
    paddingRight  : "20px",
    paddingBottom : "20px",
    paddingLeft   : "20px",
  },
  clipNoticeButton: {
    display       : "block",
    color         : "white",
    background    : "transparent",
    border        : "0",
    textDecoration: "underline",
    marginTop     : "12px",
  },
  clipPlay: {
    color         : "#fff",
    background    : "#17171780",
    border        : "1px solid #ffffff40",
    backdropFilter: "blur(16px)",
    position      : "absolute",
    top           : "42%",
    left          : "50%",
    transform     : "translate(-50%, -50%)",
    display       : "grid",
    placeItems    : "center",
    gap           : "12px",
    fontSize      : "12px",
    paddingTop    : "17px",
    paddingRight  : "17px",
    paddingBottom : "17px",
    paddingLeft   : "17px",
  },
  clipLoading: {
    color         : "#fff",
    background    : "#17171780",
    border        : "1px solid #ffffff40",
    backdropFilter: "blur(16px)",
    position      : "absolute",
    top           : "75px",
    left          : "18px",
    fontSize      : "11px",
    paddingTop    : "7px",
    paddingRight  : "10px",
    paddingBottom : "7px",
    paddingLeft   : "10px",
  },
  clipCaption: {
    position      : "absolute",
    bottom        : { default: "20px", "@media (max-width: 760px)": "16px" },
    left          : "18px",
    right         : "18px",
    background    : "#15151566",
    borderTop     : "1px solid #ffffff4d",
    backdropFilter: "blur(18px)",
    paddingTop    : "15px",
    paddingRight  : "17px",
    paddingBottom : "15px",
    paddingLeft   : "17px",
  },
  clipTitle: {
    fontSize     : "23px",
    fontWeight   : "500",
    letterSpacing: "-0.6px",
  },
  clipDescription: {
    fontSize  : "12px",
    lineHeight: "1.7",
    color     : "#ffffffe0",
    marginTop : "6px",
  },
  clipCredit: {
    display  : "inline-block",
    fontSize : "10px",
    color    : "#ffffffbd",
    marginTop: "14px",
    ":hover": {
      color: "white",
    },
  },
  clipCreditMark: {
    marginLeft: "5px",
  },
  clipsLayout: {
    position      : "relative",
    display       : { default: "flex", "@media (max-width: 760px)": "block" },
    justifyContent: "center",
    gap           : "28px",
  },
  clipsPlayer: {
    position: "relative",
    minWidth: "0",
  },
  clipsBar: {
    position      : "absolute",
    top           : "18px",
    left          : "18px",
    zIndex        : "2",
    display       : "flex",
    alignItems    : "center",
    gap           : "14px",
    height        : "42px",
    color         : "#fff",
    background    : "#17171770",
    backdropFilter: "blur(16px)",
    pointerEvents : "none",
    paddingTop    : "0",
    paddingRight  : "13px",
    paddingBottom : "0",
    paddingLeft   : "13px",
  },
  clipsTitle: {
    fontSize     : "14px",
    fontWeight   : "500",
    letterSpacing: "-0.2px",
  },
  clipsCount: {
    fontSize          : "10px",
    fontVariantNumeric: "tabular-nums",
  },
  clipsCountTotal: {
    color: "#ffffff99",
  },
  clipsViewport: {
    height: {
      default                    : "calc(100svh - 48px)",
      "@media (max-width: 760px)": "calc(100svh - 58px - env(safe-area-inset-bottom))",
    },
    minHeight          : { default: "380px", "@media (max-width: 760px)": "280px" },
    maxHeight          : { default: "900px", "@media (max-width: 760px)": "none" },
    aspectRatio        : { default: "9 / 16", "@media (max-width: 760px)": "auto" },
    overflowY          : "auto",
    scrollbarWidth     : "none",
    scrollSnapType     : "y mandatory",
    overscrollBehaviorY: "contain",
    background         : "#171717",
    width              : { "@media (max-width: 760px)": "100%" },
    "::-webkit-scrollbar": {
      display: "none",
    },
  },
  clipSlide: {
    height         : "100%",
    width          : "100%",
    scrollSnapAlign: "start",
    scrollSnapStop : "always",
  },
  clipsNavigation: {
    display      : { default: "flex", "@media (max-width: 760px)": "none" },
    flexDirection: "column",
    alignSelf    : "center",
    alignItems   : "center",
    gap          : "12px",
    width        : "60px",
  },
  clipsNavButton: {
    display     : "grid",
    placeItems  : "center",
    width       : "44px",
    height      : "44px",
    background  : "#ffffff70",
    border      : "1px solid #ffffff",
    borderRadius: "50%",
    ":disabled": {
      opacity: "0.3",
      cursor : "default",
    },
  },
});
