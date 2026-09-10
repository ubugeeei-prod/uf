// @flow

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { type View, pageTitle, viewHref, viewLabel } from "./social-model.js";

export component NavItem(view: View, active: boolean = false) renders React.Node {
  return (
    <Link
      to={viewHref(view)}
      aria-current={active ? "page" : undefined}
      {...props(styles.navItem, active && styles.navItemActive)}
    >
      {viewLabel(view)}
    </Link>
  );
}

component PrimaryNav(kind: "top" | "tab" = "top", children: renders* NavItem) renders React.Node {
  return (
    <nav
      aria-label={kind === "top" ? "Primary navigation" : "Primary tabs"}
      {...props(kind === "top" ? styles.nav : styles.tabNav)}
    >
      {children}
    </nav>
  );
}

export component SocialFrame(
  active: View,
  children: React.Node,
  aside?: React.Node,
) renders React.Node {
  return (
    <main {...props(styles.shell)}>
      <header {...props(styles.header)}>
        <div {...props(styles.brandBlock)}>
          <Link to="/" {...props(styles.brand)}>
            uf social
          </Link>
          <p {...props(styles.kicker)}>SSR example</p>
        </div>
        <PrimaryNav kind="top">
          <NavItem view="timeline" active={active === "timeline"} />
          <NavItem view="messages" active={active === "messages"} />
          <NavItem view="settings" active={active === "settings"} />
          <NavItem view="login" active={active === "login"} />
          <NavItem view="signup" active={active === "signup"} />
        </PrimaryNav>
      </header>
      <section {...props(styles.hero)}>
        <p {...props(styles.eyebrow)}>React 19.3 / RSC / Actions / SQLite</p>
        <h1 {...props(styles.title)}>{pageTitle(active)}</h1>
      </section>
      <div {...props(styles.content, aside != null && styles.contentWithAside)}>
        <section {...props(styles.primary)}>{children}</section>
        {aside == null ? null : <aside {...props(styles.aside)}>{aside}</aside>}
      </div>
      <PrimaryNav kind="tab">
        <NavItem view="timeline" active={active === "timeline"} />
        <NavItem view="messages" active={active === "messages"} />
        <NavItem view="settings" active={active === "settings"} />
        <NavItem view="login" active={active === "login"} />
        <NavItem view="signup" active={active === "signup"} />
      </PrimaryNav>
    </main>
  );
}

const styles = stylex.create({
  shell: {
    minHeight: "100vh",
    backgroundColor: "#edf2f7",
    color: "#0f172a",
    fontFamily: ufTokens.fontSans,
    paddingBottom: {
      default: 86,
      "@media (min-width: 760px)": 0,
    },
  },
  header: {
    alignItems: "center",
    backdropFilter: "blur(18px)",
    backgroundColor: "rgba(255, 255, 255, 0.86)",
    borderBottomColor: "#d8e0ea",
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "flex",
    gap: 16,
    justifyContent: "space-between",
    marginInline: "auto",
    maxWidth: 1180,
    paddingBlock: 12,
    paddingInline: {
      default: 16,
      "@media (min-width: 760px)": 28,
    },
    position: "sticky",
    top: 0,
    zIndex: 20,
  },
  brandBlock: {
    display: "grid",
    gap: 2,
  },
  brand: {
    color: "#111827",
    fontSize: 19,
    fontWeight: 800,
    textDecorationLine: "none",
  },
  kicker: {
    color: "#667085",
    fontSize: 12,
    marginBlock: 0,
    textTransform: "uppercase",
  },
  nav: {
    alignItems: "center",
    display: {
      default: "none",
      "@media (min-width: 760px)": "flex",
    },
    flexWrap: "wrap",
    gap: 8,
    justifyContent: "flex-end",
  },
  navItem: {
    borderColor: "transparent",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    color: "#344054",
    fontSize: 14,
    fontWeight: 700,
    paddingBlock: 8,
    paddingInline: {
      default: 10,
      "@media (min-width: 760px)": 12,
    },
    textDecorationLine: "none",
  },
  navItemActive: {
    backgroundColor: "#111827",
    borderColor: "#111827",
    color: "#ffffff",
  },
  hero: {
    display: "grid",
    gap: 6,
    marginInline: "auto",
    maxWidth: 1180,
    paddingBlock: {
      default: 18,
      "@media (min-width: 760px)": 28,
    },
    paddingInline: {
      default: 16,
      "@media (min-width: 760px)": 28,
    },
  },
  eyebrow: {
    color: "#2563eb",
    fontSize: 13,
    fontWeight: 800,
    letterSpacing: 0,
    marginBlock: 0,
  },
  title: {
    color: "#101828",
    fontSize: {
      default: 30,
      "@media (min-width: 760px)": 40,
    },
    lineHeight: 1.08,
    marginBlock: 0,
  },
  content: {
    display: "grid",
    gap: {
      default: 14,
      "@media (min-width: 760px)": 24,
    },
    gridTemplateColumns: "minmax(0, 1fr)",
    marginInline: "auto",
    maxWidth: 1160,
    paddingBlock: {
      default: 0,
      "@media (min-width: 760px)": 24,
    },
    paddingInline: {
      default: 12,
      "@media (min-width: 760px)": 24,
    },
  },
  contentWithAside: {
    gridTemplateColumns: {
      default: "minmax(0, 1fr)",
      "@media (min-width: 960px)": "minmax(0, 1fr) 320px",
    },
  },
  primary: {
    minWidth: 0,
  },
  aside: {
    minWidth: 0,
  },
  tabNav: {
    alignItems: "center",
    backdropFilter: "blur(20px)",
    backgroundColor: "rgba(255, 255, 255, 0.9)",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    bottom: 12,
    boxShadow: "0 16px 48px rgba(15, 23, 42, 0.18)",
    display: {
      default: "grid",
      "@media (min-width: 760px)": "none",
    },
    gap: 4,
    gridTemplateColumns: "repeat(5, minmax(0, 1fr))",
    left: 12,
    padding: 6,
    position: "fixed",
    right: 12,
    zIndex: 30,
  },
});
