// @flow

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { Avatar, Icon } from "./ui.js";
import { SignOut } from "./session-client.js";
import {
  TOPICS,
  topicLabel,
  feedHref,
  GUEST,
  type Session,
  type User,
  type View,
} from "./social-model.js";

const PEOPLE: $ReadOnlyArray<User> = [
  { id: "seed-mika", name: "Mika Tan", handle: "mika", avatar: "MT", bio: "Product engineering" },
  { id: "seed-ren", name: "Ren Ito", handle: "ren", avatar: "RI", bio: "Design systems" },
  { id: "seed-sora", name: "Sora Lin", handle: "sora", avatar: "SL", bio: "Developer tools" },
  { id: "seed-niko", name: "Niko Reyes", handle: "niko", avatar: "NR", bio: "Community" },
];

/**
 * Compose the shared navigation and route content, with a compact shell for the immersive Clips view.
 */
export component SocialFrame(
  active: View,
  session: Session = GUEST,
  aside: boolean = true,
  ...{ children }: React.ElementConfig<"main">
) {
  const links = [
    { view: "timeline", href: "/", icon: "home", label: "Feed" },
    { view: "clips", href: "/clips", icon: "video", label: "Clips" },
    { view: "messages", href: "/messages", icon: "message", label: "Inbox" },
    { view: "settings", href: "/settings", icon: "settings", label: "Settings" },
  ];

  return (
    <div className="app-shell">
      <a className="skip-link" href="#main-content">
        Skip to content
      </a>
      <aside className="sidebar">
        <Link className="brand" to="/" aria-label="Commonplace home">
          Commonplace
        </Link>
        <div className="workspace-label">
          <span className="online-dot" />
          Community workspace
        </div>
        <nav className="main-nav" aria-label="Primary navigation">
          {links.map((link) => (
            <Link
              key={link.view}
              to={link.href}
              aria-current={active === link.view ? "page" : undefined}
            >
              <Icon name={link.icon} size={18} />
              <span>{link.label}</span>
            </Link>
          ))}
        </nav>
        <div className="channel-nav">
          <p className="eyebrow">Channels</p>
          {TOPICS.map((topic) => (
            <Link to={feedHref(topic)} key={topic}>
              <Icon name="hash" size={16} />
              {topicLabel(topic)}
            </Link>
          ))}
        </div>
        <Link
          className="compose-link"
          to={
            match (session) {
              {kind: "guest"} => "/login",
              {kind: "authenticated", ...} => "/#compose",
            }
          }
        >
          <Icon name="compose" size={16} />
          Write a note
        </Link>
        <div className="account">
          {
            match (session) {
              {kind: "authenticated", user: const user} =>
                <>
                  <Link to="/settings" className="account-person">
                    <Avatar user={user} small />
                    <span>
                      <strong>{user.name}</strong>
                      <small>@{user.handle}</small>
                    </span>
                  </Link>
                  <SignOut />
                </>,
              {kind: "guest"} =>
                <>
                  <Link className="button primary" to="/login">
                    Sign in
                  </Link>
                  <Link className="button secondary" to="/signup">
                    Join
                  </Link>
                </>,
            }
          }
        </div>
      </aside>
      <div
        className={`workspace ${active === "timeline" ? "feed-workspace" : ""} ${aside ? "with-aside" : "wide"} ${active === "clips" ? "clips-workspace" : active === "login" || active === "signup" ? "account-workspace" : ""}`}
      >
        <header className="topbar">
          <span className="breadcrumb">
            <Icon
              name={
                match (active) {
                  "timeline" => "home",
                  "clips" => "video",
                  "messages" => "message",
                  "settings" | "login" | "signup" => "settings",
                }
              }
              size={15}
            />
            Commonplace<span>/</span>
            <strong>
              {
                match (active) {
                  "timeline" => "Feed",
                  "clips" => "Clips",
                  "messages" => "Inbox",
                  "settings" => "Settings",
                  "login" | "signup" => "Account",
                }
              }
            </strong>
          </span>
          <Link className="topbar-action" to="/?topic=community">
            <Icon name="users" size={15} />
            Community
          </Link>
          <Link className="mobile-brand" to="/">
            Commonplace
          </Link>
        </header>
        <main id="main-content" className="main-column" tabIndex={-1}>
          {children}
        </main>
        {aside ? (
          <aside className="discovery" aria-label="Community directory">
            <section className="directory">
              <div className="section-heading">
                <h2>People in this space</h2>
                <Icon name="users" size={15} />
              </div>
              {PEOPLE.map((person) => (
                <Link
                  className="directory-person"
                  key={person.id}
                  to={feedHref("all", person.handle)}
                >
                  <Avatar user={person} small />
                  <span>
                    <strong>{person.name}</strong>
                    <small>{person.bio}</small>
                  </span>
                  <Icon name="arrow" size={14} />
                </Link>
              ))}
            </section>
            <section className="channel-directory">
              <div className="section-heading">
                <h2>Explore channels</h2>
                <Icon name="hash" size={15} />
              </div>
              {TOPICS.map((topic) => (
                <Link key={topic} to={feedHref(topic)}>
                  <span className={`channel-dot ${topic}`} />
                  <span>{topicLabel(topic)}</span>
                  <Icon name="arrow" size={13} />
                </Link>
              ))}
            </section>
            <footer className="discovery-footer">
              <span>Commonplace</span>
              <span>Local workspace</span>
            </footer>
          </aside>
        ) : null}
      </div>
      <nav className="mobile-nav" aria-label="Mobile navigation">
        {links.map((link) => (
          <Link
            key={link.view}
            to={link.href}
            aria-current={active === link.view ? "page" : undefined}
          >
            <Icon name={link.icon} size={20} />
            <span>{link.label}</span>
          </Link>
        ))}
        <Link
          to={
            match (session) {
              {kind: "guest"} => "/login",
              {kind: "authenticated", ...} => "/#compose",
            }
          }
        >
          <Icon name="compose" size={20} />
          <span>
            {
              match (session) {
                {kind: "guest"} => "Sign in",
                {kind: "authenticated", ...} => "Write",
              }
            }
          </span>
        </Link>
      </nav>
    </div>
  );
}
