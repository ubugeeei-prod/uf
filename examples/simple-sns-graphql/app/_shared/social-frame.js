// @flow

import * as React from "@uniflowed/react";
import { Suspense } from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { props, stylex } from "@uniflowed/stylex";

import { preloadSession } from "../_server/relay.server.js";

import { Avatar, Icon, styles as uiStyles } from "./ui.js";
import { MobileCompose, SessionControls, type SessionRef } from "./session.client.js";
import { TOPICS, topicLabel, feedHref, type User, type View } from "./social-model.js";

const PEOPLE: $ReadOnlyArray<User> = [
  { id: "seed-mika", name: "Mika Tan", handle: "mika", avatar: "MT", bio: "Product engineering" },
  { id: "seed-ren", name: "Ren Ito", handle: "ren", avatar: "RI", bio: "Design systems" },
  { id: "seed-sora", name: "Sora Lin", handle: "sora", avatar: "SL", bio: "Developer tools" },
  { id: "seed-niko", name: "Niko Reyes", handle: "niko", avatar: "NR", bio: "Community" },
];

/**
 * Compose the shared navigation and route content, with a compact shell for the immersive Clips view.
 * The shell is a server component. Only the identity-dependent controls are client islands, and
 * both stream from one viewer preload; a route that gates on identity passes its own `session`.
 */

export component SocialFrame(
  active : View,
  aside  : boolean = true,
  session: SessionRef = preloadSession(),
  ...{ children }: React.ElementConfig<"main">
) {
  const links = [
    { view: "timeline", href: "/", icon: "home", label: "Feed" },
    { view: "clips", href: "/clips", icon: "video", label: "Clips" },
    { view: "messages", href: "/messages", icon: "message", label: "Inbox" },
    { view: "settings", href: "/settings", icon: "settings", label: "Settings" },
  ];

  return (
    <div {...props(styles.appShell)}>
      <a {...props(styles.skipLink)} href="#main-content">
        Skip to content
      </a>
      <aside {...props(styles.sidebar)}>
        <Link {...props(styles.brand)} to="/" aria-label="Commonplace home">
          Commonplace
        </Link>
        <div {...props(styles.workspaceLabel)}>
          <span {...props(styles.onlineDot)} />
          Community workspace
        </div>
        <nav {...props(styles.mainNav)} aria-label="Primary navigation">
          {links.map((link) => (
            <Link
              {...props(styles.mainNavLink)}
              key={link.view}
              to={link.href}
              aria-current={
                match (active === link.view) {
                  true  => "page",
                  false => undefined,
                }
              }
            >
              <Icon name={link.icon} size={18} />
              <span>{link.label}</span>
            </Link>
          ))}
        </nav>
        <div {...props(styles.channelNav)}>
          <p {...props(styles.eyebrow)}>Channels</p>
          {TOPICS.map((topic) => (
            <Link {...props(styles.channelLink)} to={feedHref(topic)} key={topic}>
              <Icon name="hash" size={16} {...props(styles.channelIcon)} />
              {topicLabel(topic)}
            </Link>
          ))}
        </div>
        <Suspense
          fallback={
            // The link looks the same to everyone; only where it leads waits for the viewer.
            <>
              <span {...props(uiStyles.composeLink)} aria-hidden="true">
                <Icon name="compose" size={16} />
                Write a note
              </span>
              <div {...props(uiStyles.account)} aria-hidden="true" />
            </>
          }
        >
          <SessionControls queryRef={session} />
        </Suspense>
      </aside>
      <div
        {...props(
          styles.workspace,
          aside === false && styles.wide,
          active === "clips" && styles.clipsWorkspace,
        )}
      >
        <header {...props(styles.topbar, active === "clips" && styles.clipsTopbar)}>
          <span {...props(styles.breadcrumb)}>
            <Icon
              name={
                match (active) {
                  "timeline"                      => "home",
                  "clips"                         => "video",
                  "messages"                      => "message",
                  "settings" | "login" | "signup" => "settings",
                }
              }
              size={15}
            />
            Commonplace<span>/</span>
            <strong {...props(styles.breadcrumbCurrent)}>
              {
                match (active) {
                  "timeline"         => "Feed",
                  "clips"            => "Clips",
                  "messages"         => "Inbox",
                  "settings"         => "Settings",
                  "login" | "signup" => "Account",
                }
              }
            </strong>
          </span>
          <Link {...props(styles.topbarAction)} to="/?topic=community">
            <Icon name="users" size={15} />
            Community
          </Link>
          <Link {...props(styles.mobileBrand)} to="/">
            Commonplace
          </Link>
        </header>
        <main
          id="main-content"
          {...props(
            styles.mainColumn,
            active === "timeline" && styles.feedMainColumn,
            active === "clips" && styles.clipsMainColumn,
            (active === "login" || active === "signup") && styles.accountMainColumn,
          )}
          tabIndex={-1}
        >
          {children}
        </main>
        {
          match (aside) {
            false => null,
            true  =>
              <aside {...props(styles.discovery)} aria-label="Community directory">
                <section {...props(styles.directory)}>
                  <div {...props(styles.sectionHeading)}>
                    <h2 {...props(styles.sectionTitle)}>People in this space</h2>
                    <Icon name="users" size={15} />
                  </div>
                  {PEOPLE.map((person) => (
                    <Link
                      {...props(styles.directoryPerson)}
                      key={person.id}
                      to={feedHref("all", person.handle)}
                    >
                      <Avatar user={person} small />
                      <span {...props(styles.directoryText)}>
                        <strong {...props(styles.directoryName)}>{person.name}</strong>
                        <small {...props(styles.directoryBio)}>{person.bio}</small>
                      </span>
                      <Icon name="arrow" size={14} {...props(styles.directoryIcon)} />
                    </Link>
                  ))}
                </section>
                <section {...props(styles.channelDirectory)}>
                  <div {...props(styles.sectionHeading)}>
                    <h2 {...props(styles.sectionTitle)}>Explore channels</h2>
                    <Icon name="hash" size={15} />
                  </div>
                  {TOPICS.map((topic) => (
                    <Link {...props(styles.channelDirectoryLink)} key={topic} to={feedHref(topic)}>
                      <span {...props(uiStyles.channelDot)} />
                      <span>{topicLabel(topic)}</span>
                      <Icon name="arrow" size={13} {...props(styles.channelDirectoryIcon)} />
                    </Link>
                  ))}
                </section>
                <footer {...props(styles.discoveryFooter)}>
                  <span>Commonplace</span>
                  <span>Local workspace</span>
                </footer>
              </aside>,
          }
        }
      </div>
      <nav {...props(styles.mobileNav)} aria-label="Mobile navigation">
        {links.map((link) => (
          <Link
            {...props(uiStyles.mobileNavItem)}
            key={link.view}
            to={link.href}
            aria-current={
              match (active === link.view) {
                true  => "page",
                false => undefined,
              }
            }
          >
            <Icon name={link.icon} size={20} />
            <span>{link.label}</span>
          </Link>
        ))}
        <Suspense
          fallback={
            <span {...props(uiStyles.mobileNavItem)} aria-hidden="true">
              <Icon name="compose" size={20} />
              <span>&nbsp;</span>
            </span>
          }
        >
          <MobileCompose queryRef={session} />
        </Suspense>
      </nav>
    </div>
  );
}

const styles = stylex.create({
  appShell: {
    maxWidth: "1600px",
    margin  : "auto",
    "--line": "#dcdcdc",
  },
  skipLink: {
    position     : "fixed",
    left         : "20px",
    top          : "-80px",
    zIndex       : "100",
    background   : "#fff",
    paddingTop   : "12px",
    paddingRight : "12px",
    paddingBottom: "12px",
    paddingLeft  : "12px",
    ":focus": {
      top: "12px",
    },
  },
  sidebar: {
    position      : "fixed",
    top           : "0",
    bottom        : "0",
    width         : { default: "232px", "@media (max-width: 1199px)": "210px" },
    display       : { default: "flex", "@media (max-width: 760px)": "none" },
    flexDirection : "column",
    backdropFilter: "blur(24px)",
    background    : "#ffffff4d",
    borderRight   : "1px solid #ffffffb3",
    paddingTop    : "28px",
    paddingRight  : { default: "22px", "@media (max-width: 1199px)": "18px" },
    paddingBottom : "22px",
    paddingLeft   : { default: "22px", "@media (max-width: 1199px)": "18px" },
  },
  brand: {
    display      : "flex",
    alignItems   : "center",
    fontSize     : { default: "20px", "@media (max-width: 1199px)": "16px" },
    fontWeight   : "500",
    letterSpacing: "-1px",
    gap          : "0",
    "::after": {
      content     : "''",
      width       : "18px",
      height      : "1px",
      background  : "currentColor",
      alignSelf   : "flex-end",
      marginBottom: "5px",
      marginLeft  : "8px",
    },
  },
  mainNav: {
    display: "grid",
    gap    : "5px",
  },
  mainNavLink: {
    display      : "flex",
    alignItems   : "center",
    gap          : "12px",
    borderRadius : "5px",
    fontSize     : "13px",
    color        : "#636363",
    paddingTop   : "10px",
    paddingRight : "12px",
    paddingBottom: "10px",
    paddingLeft  : "12px",
    ":hover": {
      background: "#ffffffa6",
    },
    ":is([aria-current=page])": {
      color     : "var(--ink)",
      fontWeight: "600",
      background: "#e8e8e8b3",
    },
  },
  workspaceLabel: {
    display   : "flex",
    alignItems: "center",
    gap       : "6px",
    fontSize  : "11px",
    color     : "var(--muted)",
    margin    : "14px 0 32px",
  },
  onlineDot: {
    width       : "4px",
    height      : "4px",
    borderRadius: "50%",
    background  : "#989898",
  },
  eyebrow: {
    fontSize     : "10px",
    fontWeight   : "600",
    letterSpacing: "0.8px",
    textTransform: "uppercase",
    color        : "var(--muted)",
    marginBottom : "14px",
  },
  channelNav: {
    marginTop   : "32px",
    marginRight : "12px",
    marginBottom: "0",
    marginLeft  : "12px",
  },
  channelLink: {
    display      : "flex",
    alignItems   : "center",
    gap          : "10px",
    fontSize     : "13px",
    color        : "#6f6f6f",
    paddingTop   : "10px",
    paddingRight : "0",
    paddingBottom: "10px",
    paddingLeft  : "0",
    ":hover": {
      color: "var(--ink)",
    },
  },
  channelIcon: {
    color: "#979797",
  },
  workspace: {
    display: { default: "grid", "@media (max-width: 760px)": "block" },
    gridTemplateColumns: {
      default                                             : "minmax(0, 720px) 260px",
      "@media (max-width: 1199px) and (min-width: 1001px)": "minmax(0, 1fr) 220px",
      "@media (max-width: 1000px)"                        : "minmax(0, 1fr)",
    },
    justifyContent: "center",
    columnGap     : { default: "26px", "@media (max-width: 1199px)": "22px" },
    paddingTop    : "0",
    paddingRight: {
      default                                             : "32px",
      "@media (max-width: 1199px) and (min-width: 1001px)": "24px",
      "@media (max-width: 1000px) and (min-width: 761px)" : "30px",
      "@media (max-width: 760px)"                         : "12px",
    },
    paddingBottom: "0",
    paddingLeft: {
      default                                             : "32px",
      "@media (max-width: 1199px) and (min-width: 1001px)": "24px",
      "@media (max-width: 1000px) and (min-width: 761px)" : "30px",
      "@media (max-width: 760px)"                         : "12px",
    },
    marginLeft: {
      default                                            : "232px",
      "@media (max-width: 1199px) and (min-width: 761px)": "210px",
      "@media (max-width: 760px)"                        : "0",
    },
  },
  wide: {
    gridTemplateColumns: "minmax(0, 1020px)",
    display            : { default: "grid", "@media (max-width: 760px)": "block" },
    paddingTop         : "0",
    paddingRight: {
      default                                             : "32px",
      "@media (max-width: 1199px) and (min-width: 1001px)": "24px",
      "@media (max-width: 1000px) and (min-width: 761px)" : "30px",
      "@media (max-width: 760px)"                         : "12px",
    },
    paddingBottom: "0",
    paddingLeft: {
      default                                             : "32px",
      "@media (max-width: 1199px) and (min-width: 1001px)": "24px",
      "@media (max-width: 1000px) and (min-width: 761px)" : "30px",
      "@media (max-width: 760px)"                         : "12px",
    },
    marginLeft: {
      default                                            : "232px",
      "@media (max-width: 1199px) and (min-width: 761px)": "210px",
      "@media (max-width: 760px)"                        : "0",
    },
  },
  clipsWorkspace: {
    paddingRight: {
      default                                             : "32px",
      "@media (max-width: 1199px) and (min-width: 1001px)": "24px",
      "@media (max-width: 1000px) and (min-width: 761px)" : "30px",
      "@media (max-width: 760px)"                         : "0",
    },
    paddingLeft: {
      default                                             : "32px",
      "@media (max-width: 1199px) and (min-width: 1001px)": "24px",
      "@media (max-width: 1000px) and (min-width: 761px)" : "30px",
      "@media (max-width: 760px)"                         : "0",
    },
  },
  topbar: {
    gridColumn       : "1 / -1",
    display          : "flex",
    alignItems       : "center",
    justifyContent   : "space-between",
    gap              : "20px",
    height           : { default: "65px", "@media (max-width: 760px)": "48px" },
    borderBottom     : "1px solid var(--line)",
    position         : "sticky",
    top              : "0",
    zIndex           : "10",
    borderBottomColor: "#dddddd",
    backdropFilter   : "blur(20px)",
    background       : "#edededcc",
    paddingRight     : { "@media (max-width: 760px)": "6px" },
    paddingLeft      : { "@media (max-width: 760px)": "6px" },
  },
  clipsTopbar: {
    display    : "none",
    marginRight: { "@media (max-width: 760px)": "18px" },
    marginLeft : { "@media (max-width: 760px)": "18px" },
  },
  breadcrumb: {
    display   : { default: "flex", "@media (max-width: 760px)": "none" },
    alignItems: "center",
    gap       : "10px",
    fontSize  : "12px",
    color     : "var(--muted)",
  },
  breadcrumbCurrent: {
    fontWeight: "500",
    color     : "var(--ink)",
  },
  topbarAction: {
    display   : { default: "flex", "@media (max-width: 760px)": "none" },
    alignItems: "center",
    gap       : "7px",
    fontSize  : "12px",
    color     : "var(--muted)",
    ":hover": {
      color: "var(--ink)",
    },
  },
  mobileBrand: {
    display      : { default: "none", "@media (max-width: 760px)": "block" },
    fontSize     : { "@media (max-width: 760px)": "17px" },
    fontWeight   : { "@media (max-width: 760px)": "650" },
    letterSpacing: { "@media (max-width: 760px)": "-0.5px" },
  },
  mainColumn: {
    minWidth      : "0",
    background    : "transparent",
    border        : "0",
    borderRadius  : "0",
    backdropFilter: "none",
    paddingTop: {
      default                                            : "26px",
      "@media (max-width: 1199px) and (min-width: 761px)": "21px",
      "@media (max-width: 760px)"                        : "18px",
    },
    paddingRight: { default: "0", "@media (max-width: 1199px)": "6px" },
    paddingBottom: {
      default                                            : "40px",
      "@media (max-width: 1199px) and (min-width: 761px)": "30px",
      "@media (max-width: 760px)"                        : "24px",
    },
    paddingLeft : { default: "0", "@media (max-width: 1199px)": "6px" },
    marginTop   : { default: "26px", "@media (max-width: 760px)": "0" },
    marginBottom: { default: "32px", "@media (max-width: 760px)": "76px" },
  },
  feedMainColumn: {
    paddingTop: {
      default                                            : "26px",
      "@media (max-width: 1199px) and (min-width: 761px)": "21px",
      "@media (max-width: 760px)"                        : "0",
    },
  },
  clipsMainColumn: {
    paddingTop: { default: "24px", "@media (max-width: 760px)": "0" },
    paddingRight: {
      default                                            : "0",
      "@media (max-width: 1199px) and (min-width: 761px)": "6px",
      "@media (max-width: 760px)"                        : "0",
    },
    paddingBottom: { default: "24px", "@media (max-width: 760px)": "0" },
    paddingLeft: {
      default                                            : "0",
      "@media (max-width: 1199px) and (min-width: 761px)": "6px",
      "@media (max-width: 760px)"                        : "0",
    },
    marginTop   : "0",
    marginBottom: "0",
    marginRight : { "@media (max-width: 760px)": "0" },
    marginLeft  : { "@media (max-width: 760px)": "0" },
  },
  accountMainColumn: {
    background    : "transparent",
    border        : "0",
    backdropFilter: "none",
  },
  discovery: {
    paddingTop: "26px",
    display   : { "@media (max-width: 1000px)": "none" },
  },
  directory: {
    background    : "transparent",
    border        : "0",
    borderRadius  : "0",
    backdropFilter: "none",
    paddingTop    : "0",
    paddingRight  : "0",
    paddingBottom : "0",
    paddingLeft   : "0",
    marginBottom  : "32px",
  },
  channelDirectory: {
    background    : "transparent",
    border        : "0",
    borderRadius  : "0",
    backdropFilter: "none",
    paddingTop    : "0",
    paddingRight  : "0",
    paddingBottom : "0",
    paddingLeft   : "0",
    marginBottom  : "32px",
  },
  sectionHeading: {
    display       : "flex",
    alignItems    : "center",
    justifyContent: "space-between",
    color         : "#929292",
    marginBottom  : "12px",
  },
  sectionTitle: {
    fontSize  : "12px",
    fontWeight: "600",
    color     : "#595959",
  },
  directoryPerson: {
    display      : "flex",
    alignItems   : "center",
    gap          : "10px",
    paddingTop   : "10px",
    paddingRight : "0",
    paddingBottom: "10px",
    paddingLeft  : "0",
  },
  directoryText: {
    flex    : "1",
    minWidth: "0",
  },
  directoryName: {
    fontSize  : "12px",
    fontWeight: "550",
    display   : "block",
  },
  directoryBio: {
    fontSize : "11px",
    color    : "var(--muted)",
    display  : "block",
    marginTop: "4px",
  },
  directoryIcon: {
    color: "#ababab",
  },
  channelDirectoryLink: {
    display      : "flex",
    gap          : "10px",
    alignItems   : "center",
    fontSize     : "12px",
    color        : "#6b6b6b",
    paddingTop   : "11px",
    paddingRight : "0",
    paddingBottom: "11px",
    paddingLeft  : "0",
    ":hover": {
      color: "var(--ink)",
    },
  },
  channelDirectoryIcon: {
    color     : "#ababab",
    marginLeft: "auto",
  },
  discoveryFooter: {
    display      : "grid",
    gap          : "5px",
    lineHeight   : "1.7",
    color        : "var(--muted)",
    fontSize     : "10px",
    paddingInline: "4px",
  },
  mobileNav: {
    display            : { default: "none", "@media (max-width: 760px)": "grid" },
    position           : { "@media (max-width: 760px)": "fixed" },
    zIndex             : { "@media (max-width: 760px)": "20" },
    bottom             : { "@media (max-width: 760px)": "0" },
    left               : { "@media (max-width: 760px)": "0" },
    right              : { "@media (max-width: 760px)": "0" },
    gridTemplateColumns: { "@media (max-width: 760px)": "repeat(5, 1fr)" },
    borderTop          : { "@media (max-width: 760px)": "1px solid var(--line)" },
    background         : { "@media (max-width: 760px)": "#f8f8f8d9" },
    borderTopColor     : { "@media (max-width: 760px)": "#fff" },
    backdropFilter     : { "@media (max-width: 760px)": "blur(24px)" },
    paddingTop         : { "@media (max-width: 760px)": "9px" },
    paddingRight       : { "@media (max-width: 760px)": "12px" },
    paddingBottom      : { "@media (max-width: 760px)": "calc(9px + env(safe-area-inset-bottom))" },
    paddingLeft        : { "@media (max-width: 760px)": "12px" },
  },
});
