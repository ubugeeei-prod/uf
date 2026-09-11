"use client";
// @flow
import * as React from "@uniflowed/react";
import { useEffect, useRef, useState } from "@uniflowed/react";
import { Icon } from "../ui.js";
import type { Clip, Playback } from "./clip-model.js";

// These effects synchronize browser media and visibility, not application data.
export component ClipPlayer(clip: Clip, active: boolean, muted: boolean, onMute: () => void) {
  const video = useRef<HTMLVideoElement | null>(null);
  const [playback, setPlayback] = useState<Playback>({ kind: "paused" });
  useEffect(() => {
    const player = video.current;
    if (player == null || !active) return;
    const lifetime = new AbortController();
    const play = (): void => {
      player.play().catch(() => {
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
    if (player.paused) player.play().catch(() => setPlayback({ kind: "blocked" }));
    else player.pause();
  }
  const playing = playback.kind === "playing";
  return (
    <div className="clip-stage">
      <video
        ref={video}
        src={active ? clip.src : undefined}
        poster={clip.poster}
        preload={active ? "auto" : "none"}
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
      {active ? (
        <>
          <div className="clip-controls">
            <button
              type="button"
              onClick={toggle}
              aria-label={playing ? "Pause video" : "Play video"}
            >
              <Icon name={playing ? "pause" : "play"} size={19} />
            </button>
            <button
              type="button"
              onClick={onMute}
              aria-label={muted ? "Unmute video" : "Mute video"}
              aria-pressed={!muted}
            >
              <Icon name={muted ? "muted" : "volume"} size={19} />
            </button>
          </div>
          {
            match (playback) {
              {kind: "error"} =>
                <div className="clip-notice" role="alert">
                  Could not load this clip.{" "}
                  <button
                    type="button"
                    onClick={() => {
                      video.current?.load();
                      toggle();
                    }}
                  >
                    Try again
                  </button>
                </div>,
              {kind: "blocked"} =>
                <button type="button" className="clip-play" onClick={toggle}>
                  <Icon name="play" size={26} />
                  <span>Play video</span>
                </button>,
              {kind: "loading"} =>
                <span className="clip-loading" role="status">
                  Loading video…
                </span>,
              {kind: "paused"} | {kind: "playing"} => null,
            }
          }
        </>
      ) : null}
      <div className="clip-caption">
        <h2>{clip.title}</h2>
        <p>{clip.description}</p>
        <a href={clip.source} target="_blank" rel="noreferrer">
          Film by {clip.credit} <span>↗</span>
        </a>
      </div>
    </div>
  );
}

export component Clips(clips: $ReadOnlyArray<Clip>) {
  const viewport = useRef<HTMLDivElement | null>(null);
  const [selected, setSelected] = useState(0);
  const [muted, setMuted] = useState(true);
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
      top: index * element.clientHeight,
      behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth",
    });
  }
  return (
    <div className="clips-layout">
      <div className="clips-player">
        <header className="clips-bar">
          <h1>Clips</h1>
          <span aria-live="polite">
            {String(selected + 1).padStart(2, "0")}
            <span> / {String(clips.length).padStart(2, "0")}</span>
          </span>
        </header>
        <div
          ref={viewport}
          className="clips-viewport"
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
              className="clip-slide"
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
      <nav className="clips-navigation" aria-label="Video navigation">
        <button
          type="button"
          onClick={() => move(selected - 1)}
          disabled={selected === 0}
          aria-label="Previous clip"
        >
          <Icon name="chevron-up" />
        </button>
        <button
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
