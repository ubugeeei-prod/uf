// @flow

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { props, stylex } from "@uniflowed/stylex";
import { SkeletonBox } from "@uniflowed/ui/skeleton";
import { avatarPhoto, type User } from "./social-model.js";
import { AvatarRoot, AvatarImage, AvatarFallback } from "@uniflowed/ui/avatar";

/**
 * Render decorative line icons on a shared grid; the owning control supplies its accessible name.
 */
export component Icon(name: string, size: number = 20) {
  const paths: { [string]: string } = {
    video: "M7 3h10a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Zm3 5 5 4-5 4Z",
    play: "m8 4 12 8-12 8Z",
    pause: "M8 5v14M16 5v14",
    muted: "m11 5-5 4H2v6h4l5 4ZM16 9l6 6m0-6-6 6",
    volume: "m11 5-5 4H2v6h4l5 4ZM15 8a6 6 0 0 1 0 8M18 5a10 10 0 0 1 0 14",
    "chevron-up": "m6 15 6-6 6 6",
    "chevron-down": "m6 9 6 6 6-6",
    hash: "M5 8h15M4 16h15M10 3 7 21M17 3l-3 18",
    compose:
      "M12 4H5a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h13a2 2 0 0 0 2-2v-7M16 3l5 5M10 14l-1 5 5-1L22 10l-5-5Z",
    users:
      "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M15 3a4 4 0 0 1 0 8M22 21v-2a4 4 0 0 0-3-3.9M13 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0Z",
    home: "m3 10 9-7 9 7v10a1 1 0 0 1-1 1h-5v-7H9v7H4a1 1 0 0 1-1-1Z",
    message: "M21 11.5a8.5 8.5 0 0 1-8.5 8.5H4l-1 1v-9.5a8.5 8.5 0 0 1 17 0ZM7 9h9M7 13h6",
    settings: "M4 7h16M4 17h16M8 4v6M16 14v6",
    search: "m21 21-5-5M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0Z",
    heart:
      "M20.8 4.6a5.5 5.5 0 0 0-7.8 0L12 5.7l-1.1-1.1a5.5 5.5 0 0 0-7.8 7.8L12 21l8.8-8.6a5.5 5.5 0 0 0 0-7.8Z",
    arrow: "M5 12h14m-6-6 6 6-6 6",
    plus: "M12 5v14M5 12h14",
    logout: "M9 4H4v16h5M9 12h12m-4-4 4 4-4 4",
    leaf: "M20 3C7 2 2 8 5 16c8 3 14-2 15-13ZM4 21 15 10",
    check: "m5 12 4 4L19 6",
    lock: "M6 11h12v10H6ZM8 11V7a4 4 0 0 1 8 0v4",
  };

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.65"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={paths[name] ?? paths.leaf} />
    </svg>
  );
}

/** Compose the UI avatar primitive with a licensed portrait or a stable initials fallback. */
export component Avatar(user: User, small: boolean = false) renders AvatarRoot {
  const photo = user.photo ?? avatarPhoto(user.id);

  return (
    <AvatarRoot {...props(styles.avatar, small && styles.small)} aria-hidden="true">
      {photo != null ? (
        <AvatarImage
          src={photo}
          className="avatar-photo"
          width={small ? 34 : 42}
          height={small ? 34 : 42}
          alt=""
        />
      ) : null}
      <AvatarFallback className="avatar-fallback">{user.avatar}</AvatarFallback>
    </AvatarRoot>
  );
}

/** Render a regional explanation with an optional typed recovery action. */
export component EmptyState(
  title: string,
  children: string,
  action: renders? (ActionLink | RetryButton) = null,
) {
  return (
    <div className="empty-state">
      <span className="empty-icon">
        <Icon name="search" size={24} />
      </span>
      <h2>{title}</h2>
      <p>{children}</p>
      {action}
    </div>
  );
}

/** Render navigation with the shared primary or text-link treatment. */
export component ActionLink(to: string, children: string, primary: boolean = true) {
  return (
    <Link className={primary ? "button primary" : "text-link"} to={to}>
      {children}
      <Icon name="arrow" size={15} />
    </Link>
  );
}

/** Expose an explicit retry for one failed async region. */
export component RetryButton(onRetry: () => void) {
  return (
    <button type="button" className="button primary" onClick={onRetry}>
      Try again
    </button>
  );
}

/** Explain an authentication requirement without obscuring the rest of the page. */
export component SignInPrompt(title: string = "Sign in to continue") renders EmptyState {
  return (
    <EmptyState title={title} action={<ActionLink to="/login">Sign in</ActionLink>}>
      Use your account to access your messages and profile.
    </EmptyState>
  );
}

component SkeletonField(multiline: boolean = false) {
  return (
    <div className="field" aria-hidden="true">
      <SkeletonBox className="skeleton-ink skeleton-field-label" />
      <SkeletonBox className={`skeleton-control ${multiline ? "multiline" : ""}`} />
    </div>
  );
}

/**
 * Reserve each region’s real content geometry while its own Suspense boundary waits.
 * Placeholders are decorative; one status label communicates loading to assistive technology.
 */
export component LoadingState(kind: "feed" | "threads" | "conversation" | "profile") {
  const label = match (kind) {
    "feed" => "Loading notes",
    "threads" => "Loading conversations",
    "conversation" => "Loading messages",
    "profile" => "Loading profile",
  };

  return (
    <section
      className={`loading-state loading-${kind}`}
      role="status"
      aria-label={label}
      aria-busy="true"
    >
      {
        match (kind) {
          "feed" =>
            [0, 1, 2].map((id) => (
              <div className="post skeleton-post" key={id} aria-hidden="true">
                <SkeletonBox className="skeleton-avatar" />
                <div className="post-content">
                  <div className="post-header">
                    <SkeletonBox className="skeleton-ink skeleton-author" />
                    <SkeletonBox className="skeleton-ink skeleton-handle" />
                    <SkeletonBox className="skeleton-ink skeleton-time" />
                  </div>
                  <div className="post-body">
                    <SkeletonBox className="skeleton-ink skeleton-copy" />
                    <SkeletonBox className="skeleton-ink skeleton-copy" />
                    <SkeletonBox className="skeleton-ink skeleton-copy" />
                  </div>
                  <div className="post-footer">
                    <SkeletonBox className="skeleton-ink skeleton-tag" />
                    <SkeletonBox className="skeleton-ink skeleton-reaction" />
                  </div>
                </div>
              </div>
            )),
          "threads" =>
            <div className="thread-list" aria-hidden="true">
              <h2>Your conversations</h2>
              {[0, 1, 2].map((id) => (
                <div className="thread-link" key={id}>
                  <SkeletonBox className="skeleton-avatar small" />
                  <div className="skeleton-thread-copy">
                    <SkeletonBox className="skeleton-ink skeleton-author" />
                    <SkeletonBox className="skeleton-ink skeleton-copy" />
                  </div>
                </div>
              ))}
            </div>,
          "conversation" =>
            <>
              <div className="conversation-header" aria-hidden="true">
                <SkeletonBox className="skeleton-avatar small" />
                <div>
                  <SkeletonBox className="skeleton-ink skeleton-author" />
                  <SkeletonBox className="skeleton-ink skeleton-handle" />
                </div>
              </div>
              <div className="message-list" aria-hidden="true">
                {[0, 1, 2].map((id) => (
                  <div className={`message-bubble ${id === 1 ? "mine" : ""}`} key={id}>
                    <div className="skeleton-message-content">
                      <SkeletonBox className="skeleton-ink skeleton-message" />
                      <SkeletonBox className="skeleton-ink skeleton-message" />
                    </div>
                  </div>
                ))}
              </div>
              <div className="message-composer" aria-hidden="true">
                <SkeletonBox className="skeleton-ink skeleton-copy" />
                <div className="message-composer-footer">
                  <SkeletonBox className="skeleton-ink skeleton-tag" />
                  <SkeletonBox className="skeleton-ink skeleton-button" />
                </div>
              </div>
            </>,
          "profile" =>
            <div className="settings-panel" aria-hidden="true">
              <div className="settings-profile">
                <SkeletonBox className="skeleton-avatar" />
                <div>
                  <SkeletonBox className="skeleton-ink skeleton-author" />
                  <SkeletonBox className="skeleton-ink skeleton-handle" />
                </div>
              </div>
              <div className="form-section">
                <SkeletonBox className="skeleton-ink skeleton-section-title" />
                <SkeletonBox className="skeleton-ink skeleton-section-copy" />
                <div className="form-grid">
                  <SkeletonField />
                  <SkeletonField />
                </div>
                <SkeletonField multiline />
              </div>
              <div className="form-section">
                <SkeletonBox className="skeleton-ink skeleton-section-title" />
                <SkeletonBox className="skeleton-ink skeleton-section-copy" />
                <SkeletonField />
              </div>
              <div className="settings-footer">
                <SkeletonBox className="skeleton-ink skeleton-button" />
              </div>
            </div>,
        }
      }
      <span className="sr-only">{label}</span>
    </section>
  );
}
const styles = stylex.create({
  avatar: {
    position: "relative",
    overflow: "hidden",
    alignItems: "center",
    justifyContent: "center",
    display: "inline-flex",
    flexShrink: 0,
    width: 42,
    height: 42,
    borderRadius: "50%",
    backgroundColor: "#ffffffa6",
    color: "#3b3b3b",
    fontSize: 12,
    fontWeight: 550,
    border: "1px solid var(--line)",
    letterSpacing: "-0.02em",
  },
  small: { width: 34, height: 34, fontSize: 11 },
});
