// @flow

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { props, stylex, type StyleArgument } from "@uniflowed/stylex";
import { Avatar as UiAvatar, Skeleton } from "@uniflowed/ui";

import { avatarPhoto } from "./social-model.js";

/**
 * Render decorative line icons on a shared grid; the owning control supplies its accessible name.
 */

export component Icon(name: string, size: number = 20, className: string = "") {
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
    home    : "m3 10 9-7 9 7v10a1 1 0 0 1-1 1h-5v-7H9v7H4a1 1 0 0 1-1-1Z",
    message : "M21 11.5a8.5 8.5 0 0 1-8.5 8.5H4l-1 1v-9.5a8.5 8.5 0 0 1 17 0ZM7 9h9M7 13h6",
    settings: "M4 7h16M4 17h16M8 4v6M16 14v6",
    search  : "m21 21-5-5M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0Z",
    heart:
      "M20.8 4.6a5.5 5.5 0 0 0-7.8 0L12 5.7l-1.1-1.1a5.5 5.5 0 0 0-7.8 7.8L12 21l8.8-8.6a5.5 5.5 0 0 0 0-7.8Z",
    arrow : "M5 12h14m-6-6 6 6-6 6",
    plus  : "M12 5v14M5 12h14",
    logout: "M9 4H4v16h5M9 12h12m-4-4 4 4-4 4",
    leaf  : "M20 3C7 2 2 8 5 16c8 3 14-2 15-13ZM4 21 15 10",
    check : "m5 12 4 4L19 6",
    lock  : "M6 11h12v10H6ZM8 11V7a4 4 0 0 1 8 0v4",
  };

  return (
    <svg
      className={className}
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

export component Avatar(
  user : { readonly id: string, readonly photo?: ?string, readonly avatar: string, ... },
  small: boolean = false,
) renders UiAvatar.Root {
  const photo = user.photo ?? avatarPhoto(user.id);

  return (
    <UiAvatar.Root {...props(styles.avatar, small && styles.small)} aria-hidden="true">
      {
        match (photo) {
          null      => null,
          const src =>
            <UiAvatar.Image
              src={src}
              {...props(styles.avatarPhoto)}
              width={
                match (small) {
                  true  => 34,
                  false => 42,
                }
              }
              height={
                match (small) {
                  true  => 34,
                  false => 42,
                }
              }
              alt=""
            />,
        }
      }
      <UiAvatar.Fallback {...props(styles.avatarFallback)}>
        {user.avatar}
      </UiAvatar.Fallback>
    </UiAvatar.Root>
  );
}

/** Render a regional explanation with an optional typed recovery action. */

export component EmptyState(
  title   : string,
  children: string,
  action  : renders? (ActionLink | RetryButton) = null,
) {
  return (
    <div {...props(styles.emptyState)}>
      <span {...props(styles.emptyIcon)}>
        <Icon name="search" size={24} />
      </span>
      <h2 {...props(styles.emptyTitle)}>{title}</h2>
      <p {...props(styles.emptyCopy)}>{children}</p>
      {action}
    </div>
  );
}

/** Render navigation with the shared primary or text-link treatment. */

export component ActionLink(
  to       : string,
  children : string,
  primary  : boolean = true,
  xstyle   : StyleArgument = null,
  iconStyle: StyleArgument = null,
) renders Link {
  return (
    <Link
      {...match (primary) {
        true  => props(styles.button, styles.primary, xstyle),
        false => props(styles.textLink, xstyle),
      }}
      to={to}
    >
      {children}
      <Icon name="arrow" size={15} {...props(iconStyle)} />
    </Link>
  );
}

/** Expose an explicit retry for one failed async region. */

export component RetryButton(onRetry: () => void) {
  return (
    <button type="button" {...props(styles.button, styles.primary, styles.emptyAction)} onClick={onRetry}>
      Try again
    </button>
  );
}

/** Explain an authentication requirement without obscuring the rest of the page. */

export component SignInPrompt(title: string = "Sign in to continue") renders EmptyState {
  return (
    <EmptyState title={title} action={<ActionLink to="/login" xstyle={styles.emptyAction}>Sign in</ActionLink>}>
      Use your account to access your messages and profile.
    </EmptyState>
  );
}

component SkeletonField(multiline: boolean = false) {
  return (
    <div {...props(styles.field)} aria-hidden="true">
      <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonFieldLabel)} />
      <Skeleton.Box
        {...props(styles.skeletonControl, multiline && styles.skeletonControlMultiline)}
      />
    </div>
  );
}

/**
 * Reserve each region’s real content geometry while its own Suspense boundary waits.
 * Placeholders are decorative; one status label communicates loading to assistive technology.
 */

export component LoadingState(kind: "feed" | "threads" | "conversation" | "profile") {
  const label = match (kind) {
    "feed"         => "Loading notes",
    "threads"      => "Loading conversations",
    "conversation" => "Loading messages",
    "profile"      => "Loading profile",
  };

  return (
    <section
      {...props(styles.loadingState, kind === "conversation" && styles.loadingConversation)}
      role="status"
      aria-label={label}
      aria-busy="true"
    >
      {
        match (kind) {
          "feed"         =>
            [0, 1, 2].map((id) => (
              <div {...props(styles.post)} key={id} aria-hidden="true">
                <Skeleton.Box {...props(styles.skeletonAvatar)} />
                <div {...props(styles.postContent)}>
                  <div {...props(styles.postHeader)}>
                    <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonAuthor)} />
                    <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonHandle)} />
                    <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonTime)} />
                  </div>
                  <div {...props(styles.postBody)}>
                    <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonCopy)} />
                    <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonCopy)} />
                    <Skeleton.Box
                      {...props(
                        styles.skeletonInk,
                        styles.skeletonCopy,
                        id === 1 && styles.skeletonCopyWide,
                        id === 2 && styles.skeletonCopyShort,
                      )}
                    />
                  </div>
                  <div {...props(styles.postFooter)}>
                    <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonTag)} />
                    <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonReaction)} />
                  </div>
                </div>
              </div>
            )),
          "threads"      =>
            <div {...props(styles.threadList)} aria-hidden="true">
              <h2 {...props(styles.threadListHeading)}>Your conversations</h2>
              {[0, 1, 2].map((id) => (
                <div {...props(styles.threadLink)} key={id}>
                  <Skeleton.Box {...props(styles.skeletonAvatar, styles.skeletonAvatarSmall)} />
                  <div {...props(styles.skeletonThreadCopy)}>
                    <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonAuthor)} />
                    <Skeleton.Box
                      {...props(styles.skeletonInk, styles.skeletonCopy, styles.threadSkeletonCopy)}
                    />
                  </div>
                </div>
              ))}
            </div>,
          "conversation" =>
            <>
              <div {...props(styles.conversationHeader)} aria-hidden="true">
                <Skeleton.Box {...props(styles.skeletonAvatar, styles.skeletonAvatarSmall)} />
                <div>
                  <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonAuthor)} />
                  <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonHandle)} />
                </div>
              </div>
              <div {...props(styles.messageList)} aria-hidden="true">
                {[0, 1, 2].map((id) => (
                  <div {...props(styles.messageBubble, id === 1 && styles.messageMine)} key={id}>
                    <div
                      {...props(
                        styles.skeletonMessageContent,
                        id === 1 && styles.skeletonMessageMine,
                      )}
                    >
                      <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonMessage)} />
                      <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonMessage)} />
                    </div>
                  </div>
                ))}
              </div>
              <div {...props(styles.messageComposer)} aria-hidden="true">
                <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonCopy)} />
                <div {...props(styles.messageComposerFooter)}>
                  <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonTag)} />
                  <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonButton)} />
                </div>
              </div>
            </>,
          "profile"      =>
            <div {...props(styles.settingsPanel)} aria-hidden="true">
              <div {...props(styles.settingsProfile)}>
                <Skeleton.Box {...props(styles.skeletonAvatar)} />
                <div {...props(styles.profileDetails)}>
                  <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonAuthor)} />
                  <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonHandle)} />
                </div>
              </div>
              <div {...props(styles.formSection)}>
                <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonSectionTitle)} />
                <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonSectionCopy)} />
                <div {...props(styles.formGrid)}>
                  <SkeletonField />
                  <SkeletonField />
                </div>
                <SkeletonField multiline />
              </div>
              <div {...props(styles.formSection)}>
                <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonSectionTitle)} />
                <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonSectionCopy)} />
                <SkeletonField />
              </div>
              <div {...props(styles.settingsFooter, styles.loadingFooter)}>
                <Skeleton.Box {...props(styles.skeletonInk, styles.skeletonButton)} />
              </div>
            </div>,
        }
      }
      <span {...props(styles.srOnly)}>{label}</span>
    </section>
  );
}

export const styles = stylex.create({
  avatar: {
    position       : "relative",
    overflow       : "hidden",
    alignItems     : "center",
    justifyContent : "center",
    display        : "inline-flex",
    flexShrink     : 0,
    width          : 42,
    height         : 42,
    borderRadius   : "50%",
    backgroundColor: "#ffffffa6",
    color          : "#3b3b3b",
    fontSize       : 12,
    fontWeight     : 550,
    border         : "1px solid var(--line)",
    letterSpacing  : "-0.02em",
  },
  small         : { width: 34, height: 34, fontSize: 11 },
  avatarPhoto   : {
    position    : "absolute",
    inset       : "0",
    width       : "100%",
    height      : "100%",
    objectFit   : "cover",
    borderRadius: "inherit",
  },
  avatarFallback: {
    display   : "grid",
    placeItems: "center",
    width     : "100%",
    height    : "100%",
  },
  button: {
    display       : "inline-flex",
    alignItems    : "center",
    justifyContent: "center",
    gap           : "10px",
    border        : "1px solid #e3e3e3",
    borderRadius  : "6px",
    fontSize      : "12px",
    fontWeight    : "550",
    minHeight     : "38px",
    background    : "#fff",
    whiteSpace    : "nowrap",
    paddingTop    : "9px",
    paddingRight  : "15px",
    paddingBottom : "9px",
    paddingLeft   : "15px",
  },
  primary: {
    background : "var(--accent)",
    borderColor: "var(--accent)",
    color      : "#fff",
    ":hover"   : {
      background : "#131313",
      borderColor: "#131313",
    },
  },
  secondary: {
    background: "#fff",
    ":hover"  : {
      background: "var(--subtle)",
    },
  },
  textLink: {
    fontSize            : "12px",
    fontWeight          : "550",
    textDecoration      : "underline",
    textUnderlineOffset : "3px",
  },
  emptyAction: {
    marginTop: "5px",
  },
  field: {
    display      : "grid",
    gap          : "8px",
    fontSize     : "12px",
    color        : "#5f5f5f",
    marginBottom : "19px",
  },
  fieldLabel: {
    fontWeight: "550",
  },
  fieldControl: {
    width        : "100%",
    border       : "1px solid #e2e2e2",
    borderRadius : "6px",
    fontSize     : { default: "13px", "@media (max-width: 760px)": "16px" },
    color        : "var(--ink)",
    lineHeight   : "1.5",
    background   : "#ffffffc2",
    borderColor  : "#d7d7d7",
    paddingTop   : "10px",
    paddingRight : "12px",
    paddingBottom: "10px",
    paddingLeft  : "12px",
    ":is([aria-invalid=true])": {
      borderColor: "#bf6674",
    },
  },
  emptyState: {
    textAlign    : "center",
    display      : "grid",
    justifyItems : "center",
    gap          : "14px",
    borderRadius : "8px",
    background   : "transparent",
    border       : "0",
    paddingTop   : "48px",
    paddingRight : "22px",
    paddingBottom: "48px",
    paddingLeft  : "22px",
  },
  emptyTitle: {
    fontSize  : "18px",
    fontWeight: "550",
  },
  emptyCopy: {
    fontSize  : "13px",
    lineHeight: "1.7",
    color     : "var(--muted)",
    maxWidth  : "330px",
  },
  emptyIcon: {
    color       : "#868686",
    width       : "48px",
    height      : "48px",
    borderRadius: "10px",
    background  : "#efefef",
    display     : "grid",
    placeItems  : "center",
  },
  pageHeading: {
    display       : "flex",
    justifyContent: "space-between",
    alignItems    : "center",
    gap           : "15px",
    marginBottom  : { default: "22px", "@media (max-width: 760px)": "16px" },
  },
  pageTitle: {
    lineHeight   : "1.25",
    fontSize     : { default: "32px", "@media (max-width: 760px)": "24px" },
    fontWeight   : "500",
    letterSpacing: "-1.1px",
  },
  pageSummary: {
    fontSize  : "13px",
    color     : "var(--muted)",
    lineHeight: "1.65",
    marginTop : "6px",
    display   : { "@media (max-width: 760px)": "none" },
  },
  feedPageHeading: {
    marginBottom : { default: "22px", "@media (max-width: 760px)": "-1px" },
    position     : { "@media (max-width: 760px)": "absolute" },
    width        : { "@media (max-width: 760px)": "1px" },
    height       : { "@media (max-width: 760px)": "1px" },
    overflow     : { "@media (max-width: 760px)": "hidden" },
    clipPath     : { "@media (max-width: 760px)": "inset(50%)" },
    whiteSpace   : { "@media (max-width: 760px)": "nowrap" },
    paddingTop   : { "@media (max-width: 760px)": "0" },
    paddingRight : { "@media (max-width: 760px)": "0" },
    paddingBottom: { "@media (max-width: 760px)": "0" },
    paddingLeft  : { "@media (max-width: 760px)": "0" },
    marginTop    : { "@media (max-width: 760px)": "-1px" },
    marginRight  : { "@media (max-width: 760px)": "-1px" },
    marginLeft   : { "@media (max-width: 760px)": "-1px" },
  },
  post: {
    display      : "flex",
    alignItems   : "flex-start",
    gap          : { default: "14px", "@media (max-width: 760px)": "11px" },
    borderBottom : "1px solid var(--line)",
    paddingTop   : { default: "24px", "@media (max-width: 760px)": "18px" },
    paddingRight : "0",
    paddingBottom: { default: "24px", "@media (max-width: 760px)": "18px" },
    paddingLeft  : "0",
  },
  postHeader: {
    display   : "flex",
    gap       : { default: "8px", "@media (max-width: 760px)": "6px" },
    alignItems: "center",
    flexWrap  : "wrap",
    lineHeight: "1.4",
  },
  postBody: {
    fontSize    : "14px",
    lineHeight  : { default: "1.75", "@media (max-width: 760px)": "1.65" },
    color       : "#505050",
    whiteSpace  : "pre-wrap",
    overflowWrap: "anywhere",
    marginTop   : "9px",
    marginRight : "0",
    marginBottom: "14px",
    marginLeft  : "0",
  },
  postContent: {
    minWidth: "0",
    flex    : "1",
  },
  postFooter: {
    display       : "flex",
    alignItems    : "center",
    justifyContent: "space-between",
    gap           : "14px",
  },
  postError: {
    fontSize : "11px",
    color    : "#b13749",
    marginTop: "8px",
  },
  channelDot: {
    width       : "6px",
    height      : "6px",
    borderRadius: "50%",
    display     : "inline-block",
    background  : "#a0a0a0",
  },
  composeLink: {
    display      : "flex",
    alignItems   : "center",
    gap          : "10px",
    border       : "1px solid #e4e4e4",
    borderRadius : "6px",
    fontSize     : "12px",
    color        : "#585858",
    background   : "#ffffff70",
    borderColor  : "#d0d0d0",
    paddingTop   : "10px",
    paddingRight : "12px",
    paddingBottom: "10px",
    paddingLeft  : "12px",
    marginTop    : "25px",
    ":hover"     : {
      borderColor: "#bfbfbf",
    },
  },
  account: {
    display    : "flex",
    alignItems : "center",
    gap        : "8px",
    borderTop  : "1px solid var(--line)",
    paddingTop : "20px",
    marginTop  : "auto",
  },
  mobileNavItem: {
    display   : { "@media (max-width: 760px)": "grid" },
    placeItems: { "@media (max-width: 760px)": "center" },
    gap       : { "@media (max-width: 760px)": "5px" },
    // The bar is hidden above 760, so the resting color can be unconditional.
    // A media-query color would outrank :is() and the current tab would stay gray.
    color    : "#868686",
    fontSize : { "@media (max-width: 760px)": "10px" },
    minHeight: { "@media (max-width: 760px)": "40px" },
    ":is([aria-current=page])": {
      color: "var(--ink)",
    },
  },
  threadList: {
    borderRight  : { default: "1px solid var(--line)", "@media (max-width: 760px)": "0" },
    background   : "transparent",
    paddingTop   : { default: "14px", "@media (max-width: 760px)": "8px" },
    paddingRight : { default: "9px", "@media (max-width: 760px)": "8px" },
    paddingBottom: { default: "14px", "@media (max-width: 760px)": "8px" },
    paddingLeft  : { default: "9px", "@media (max-width: 760px)": "8px" },
    borderBottom : { "@media (max-width: 760px)": "1px solid var(--line)" },
    display      : { "@media (max-width: 760px)": "flex" },
    overflow     : { "@media (max-width: 760px)": "auto" },
    gap          : { "@media (max-width: 760px)": "8px" },
  },
  threadListHeading: {
    fontSize     : "11px",
    color        : "var(--muted)",
    fontWeight   : "500",
    paddingTop   : "8px",
    paddingRight : "10px",
    paddingBottom: "18px",
    paddingLeft  : "10px",
    display      : { "@media (max-width: 760px)": "none" },
  },
  threadLink: {
    display      : "flex",
    gap          : "10px",
    borderRadius : "6px",
    paddingTop   : "14px",
    paddingRight : "10px",
    paddingBottom: "14px",
    paddingLeft  : "10px",
    minWidth     : { "@media (max-width: 760px)": "205px" },
    maxWidth     : { "@media (max-width: 760px)": "245px" },
    ":hover"     : {
      background: "#f3f3f3",
    },
    ":is([aria-current=page])": {
      background: "#e7e7e7ba",
    },
  },
  conversationHeader: {
    borderBottom : "1px solid var(--line)",
    display      : "flex",
    gap          : "10px",
    alignItems   : "center",
    paddingTop   : { default: "19px", "@media (max-width: 760px)": "17px" },
    paddingRight : { default: "24px", "@media (max-width: 760px)": "17px" },
    paddingBottom: { default: "19px", "@media (max-width: 760px)": "17px" },
    paddingLeft  : { default: "24px", "@media (max-width: 760px)": "17px" },
  },
  messageList: {
    display      : "flex",
    flexDirection: "column",
    gap          : "20px",
    flex         : "1",
    maxHeight    : { default: "560px", "@media (max-width: 760px)": "400px" },
    overflowY    : "auto",
    minHeight    : { default: "320px", "@media (max-width: 760px)": "260px" },
    paddingTop   : { default: "28px", "@media (max-width: 760px)": "23px" },
    paddingRight : { default: "24px", "@media (max-width: 760px)": "17px" },
    paddingBottom: { default: "28px", "@media (max-width: 760px)": "23px" },
    paddingLeft  : { default: "24px", "@media (max-width: 760px)": "17px" },
  },
  messageBubble: {
    alignSelf: "flex-start",
    maxWidth : "87%",
  },
  messageMine: {
    alignSelf: "flex-end",
  },
  messageComposer: {
    borderTop    : "1px solid var(--line)",
    paddingTop   : { default: "16px", "@media (max-width: 760px)": "13px" },
    paddingRight : { default: "21px", "@media (max-width: 760px)": "17px" },
    paddingBottom: { default: "16px", "@media (max-width: 760px)": "13px" },
    paddingLeft  : { default: "21px", "@media (max-width: 760px)": "17px" },
  },
  messageComposerFooter: {
    display        : "flex",
    alignItems     : "center",
    justifyContent : "space-between",
    gap            : "12px",
  },
  settingsPanel: {
    maxWidth     : "740px",
    background   : "#ffffff61",
    border       : "0",
    borderRadius : "0",
    borderColor  : "#e0e0e0",
    paddingTop   : "0",
    paddingRight : "0",
    paddingBottom: "0",
    paddingLeft  : "0",
  },
  settingsProfile: {
    display      : "flex",
    alignItems   : "center",
    gap          : "13px",
    paddingBottom: "26px",
  },
  profileDetails: {
    flex: "1",
  },
  formSection: {
    borderTop   : "1px solid var(--line)",
    paddingTop  : "24px",
    marginBottom: "24px",
  },
  formGrid: {
    display            : "grid",
    gridTemplateColumns: { default: "1fr 1fr", "@media (max-width: 760px)": "1fr" },
    gap                : { default: "18px", "@media (max-width: 760px)": "0" },
  },
  settingsFooter: {
    display      : "flex",
    justifyContent: "space-between",
    alignItems   : { default: "center", "@media (max-width: 760px)": "flex-start" },
    gap          : "15px",
    borderTop    : "1px solid var(--line)",
    paddingTop   : "20px",
    flexDirection: { "@media (max-width: 760px)": "column" },
  },
  loadingFooter: {
    justifyContent: "flex-end",
  },
  loadingState: {
    maxWidth     : "none",
    minWidth     : "0",
    paddingTop   : "0",
    paddingRight : "0",
    paddingBottom: "0",
    paddingLeft  : "0",
    marginTop    : "0",
    marginRight  : "0",
    marginBottom : "0",
    marginLeft   : "0",
  },
  loadingConversation: {
    display      : "flex",
    flexDirection: "column",
    minHeight    : "580px",
  },
  skeletonAvatar: {
    width       : "42px",
    height      : "42px",
    flexShrink  : "0",
    borderRadius: "50%",
    background  : "#24242408",
    border      : "1px solid #24242406",
  },
  skeletonAvatarSmall: {
    width : "34px",
    height: "34px",
  },
  skeletonCopy: {
    height      : "7px",
    width       : "96%",
    marginTop   : "16px",
    marginBottom: "16px",
    ":last-child": {
      width: "64%",
    },
  },
  skeletonCopyWide : { ":last-child": { width: "82%" } },
  skeletonCopyShort: { ":last-child": { width: "46%" } },
  skeletonThreadCopy: {
    minWidth: "0",
    flex    : "1",
  },
  threadSkeletonCopy: {
    marginTop   : "12px",
    marginBottom: "4px",
  },
  skeletonMessage: {
    height      : "6px",
    width       : "190px",
    maxWidth    : "100%",
    marginTop   : "7px",
    marginRight : "0",
    marginBottom: "7px",
    marginLeft  : "0",
    ":last-child": {
      width: "120px",
    },
  },
  skeletonMessageContent: {
    background   : "#24242405",
    borderRadius : "0 10px 10px 10px",
    paddingTop   : "13px",
    paddingRight : "16px",
    paddingBottom: "13px",
    paddingLeft  : "16px",
  },
  skeletonMessageMine: {
    borderRadius: "10px 0 10px 10px",
  },
  skeletonControl: {
    height      : "42px",
    border      : "1px solid #2424240c",
    borderRadius: "6px",
    background  : "#ffffff33",
  },
  skeletonControlMultiline: {
    height: "88px",
  },
  skeletonInk: {
    background  : "#2424240d",
    borderRadius: "2px",
  },
  skeletonAuthor: {
    width      : "92px",
    height     : "11px",
    marginBlock: "4px",
  },
  skeletonHandle: {
    width : "48px",
    height: "9px",
  },
  skeletonTime: {
    width     : "40px",
    height    : "9px",
    marginLeft: "auto",
  },
  skeletonTag: {
    height     : "8px",
    width      : "54px",
    marginBlock: "10px",
  },
  skeletonReaction: {
    height: "10px",
    width : "28px",
  },
  skeletonFieldLabel: {
    height     : "8px",
    width      : "80px",
    marginBlock: "6px",
  },
  skeletonSectionTitle: {
    width      : "105px",
    height     : "11px",
    marginBlock: "4px 18px",
  },
  skeletonSectionCopy: {
    width       : "230px",
    maxWidth    : "85%",
    height      : "7px",
    marginBottom: "28px",
  },
  skeletonButton: {
    width       : "107px",
    height      : "36px",
    borderRadius: "6px",
  },
  srOnly: {
    position  : "absolute",
    width     : "1px",
    height    : "1px",
    padding   : "0",
    overflow  : "hidden",
    clipPath  : "inset(50%)",
    whiteSpace: "nowrap",
  },
});
