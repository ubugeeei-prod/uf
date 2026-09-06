//! `uf preview` and `uf start`: serving what `uf build` wrote.
//!
//! # Why these are two commands
//!
//! They serve the same application through the same handler — `uf build`'s
//! `dist/` for anything prerendered, `.uf/build/server/server.js` for route
//! handlers and for routes that were never prerendered — and the argument for
//! collapsing them into one command with a flag is a good one. They are two
//! because they run on two different servers, and that is a difference nobody
//! can configure away:
//!
//! - `uf preview` is **Vite's** preview server with uf's handler mounted
//!   behind it. Everything a project sets under `vite.preview` — `proxy`,
//!   `https`, `headers`, `cors` — is in effect, because the thing being
//!   checked is the build *as Vite serves it*, and red line 8 says a project
//!   that can reach Vite directly keeps reaching it. It binds loopback,
//!   because it is a local check.
//! - `uf start` is **uf's own** server over the same output, and imports Vite
//!   nowhere. A host running a production build should not need the bundler
//!   that produced it installed to answer a request; the moment it does,
//!   portable output is a claim rather than a property. It binds every
//!   interface, honours `PORT` and `HOST`, and takes the port it was given or
//!   fails.
//!
//! What is *not* allowed to differ is the answer. A preview that 404s a `POST`
//! the production server answers is worse than no preview, because a preview
//! is checked and believed — so both front doors call one handler in
//! `@uniflowed/vite`'s `internal/serve.js`, and the ordering decisions live
//! there rather than once per command.
//!
//! # The third front door
//!
//! [`super::compile`] is one as well: `uf build --compile` puts the same
//! application behind the same resolution order *inside* an executable, for a
//! host that has no JavaScript runtime to install it onto. It cannot share
//! this handler — `internal/serve.js` answers by opening files under `dist/`,
//! and a binary carries the bytes instead — so it has its own in
//! `@uniflowed/server/standalone`, kept deliberately in step with this one.
//! The rule above is what binds all three: two implementations, three
//! commands, one answer.
//!
//! # What this module does not check
//!
//! Whether the project was built. The driver refuses with the two paths it
//! looked for and the command that writes them, and it is the half that
//! computes those paths — a second copy of `dist/` and `.uf/build/server` in
//! Rust would be a second thing to keep in step with `viteConfig()`, which is
//! the drift `docs/red-lines.md` line 2 is about.

use anyhow::{Result, bail};
use camino::Utf8Path;
use uf_config::load_config;
use uf_term::{KeyValue, Status, Tone};

use crate::commands::vite::{
    Driver, Event, LogLevel, package_dir, render_error, render_log, resolve_host,
};
use crate::support::{PRODUCTION, env_file_list, plural, project_env, project_label};
use crate::ui::Ui;

/// What `uf preview` and `uf start` were asked to do.
#[derive(Debug, Clone, Default)]
pub(crate) struct ServeArgs {
    /// Bind this address instead of the command's default.
    pub(crate) host: Option<String>,
    /// Listen on this port instead of the command's default.
    pub(crate) port: Option<u16>,
    /// Run in this mode instead of `production`.
    pub(crate) mode: Option<String>,
}

/// Which of the two servers is being started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Server {
    Preview,
    Start,
}

impl Server {
    /// The driver command, which is also the `uf` subcommand's name.
    fn command(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::Start => "start",
        }
    }
}

/// Serve the build through Vite's preview server.
pub(crate) fn preview(cwd: &Utf8Path, ui: &mut Ui, args: ServeArgs) -> Result<()> {
    serve(cwd, ui, args, Server::Preview)
}

/// Serve the build through uf's own server.
pub(crate) fn start(cwd: &Utf8Path, ui: &mut Ui, args: ServeArgs) -> Result<()> {
    serve(cwd, ui, args, Server::Start)
}

fn serve(cwd: &Utf8Path, ui: &mut Ui, args: ServeArgs, which: Server) -> Result<()> {
    let resolved = load_config(cwd)?;
    let root = resolved.root.clone();

    // The same rule `uf dev --host` enforces, and only for `preview`, because
    // it is the same server: Vite validates the `Host` header of a preview
    // against `preview.allowedHosts`, which falls back to `server.allowedHosts`
    // — so a `--host` with nothing allowed starts a server that refuses every
    // request, which is worse than refusing to start. See docs/security.md.
    //
    // `uf start` is deliberately not covered. `dev.allowedHosts` is a
    // development setting, the attack it exists for is DNS rebinding against a
    // server that serves source and an HMR socket, and a production server
    // that could not bind `0.0.0.0` without a development option set would be
    // unusable in every container there is.
    if which == Server::Preview
        && args
            .host
            .as_deref()
            .is_some_and(|host| host != "127.0.0.1" && host != "localhost")
        && resolved.config.dev.allowed_hosts.is_empty()
    {
        bail!(
            "`uf preview --host` exposes the preview server to the network, which needs a \
             non-empty `dev.allowedHosts` in uf.config.js"
        );
    }

    let host = resolve_host(&resolved.config)?;
    let package = package_dir(&root)?;
    // Loaded here rather than inherited from the build: these serve a `dist/`
    // that may have been built on another machine days ago, and a server that
    // could not be pointed at a different database than the build ran against
    // would not be a server anybody could deploy.
    let env = project_env(&resolved, args.mode.as_deref(), PRODUCTION)?;
    let mut driver = Driver::spawn(
        &host,
        &package,
        &root,
        which.command(),
        &driver_args(
            args.host.as_deref(),
            args.port,
            &resolved.config.build.out_dir,
        ),
        &env,
    )?;

    let host_name = host.name();
    let project = project_label(&root).to_string();
    let banner = format!("uf {}", which.command());
    let serves = match which {
        Server::Preview => "the production build, through Vite's preview server",
        Server::Start => "the production build, with no bundler in the process",
    };
    let mode = env.mode().to_owned();
    let env_files = env_file_list(&root, &env);
    ui.render(|renderer, out| {
        renderer.banner(out, &banner, Some(&project));
        renderer.blank(out);
        let mut rows = vec![
            KeyValue::new("serving", serves),
            KeyValue::toned("host", host_name, Tone::Muted),
            KeyValue::new("mode", &mode),
        ];
        if let Some(files) = &env_files {
            rows.push(KeyValue::toned("env files", files, Tone::Path));
        }
        renderer.key_values(out, 2, &rows);
    });

    while let Some(event) = driver.next_event()? {
        match event {
            Event::Listening {
                local,
                network,
                routes,
                handlers,
            } => {
                let route_count = plural(routes.len(), "route");
                let handler_count = plural(handlers.len(), "route handler");
                ui.render(|renderer, out| {
                    renderer.blank(out);
                    for url in &local {
                        renderer.key_values(out, 2, &[KeyValue::toned("local", url, Tone::Accent)]);
                    }
                    for url in &network {
                        renderer.key_values(out, 2, &[KeyValue::toned("network", url, Tone::Warn)]);
                    }
                    renderer.key_values(
                        out,
                        2,
                        &[
                            KeyValue::toned("routes", &route_count, Tone::Number),
                            // Printed even when it is zero. A route handler
                            // that works in `uf dev` and disappears in a build
                            // is the failure this command was written for, so
                            // the count is the first thing to look at when it
                            // is wrong.
                            KeyValue::toned("handlers", &handler_count, Tone::Number),
                        ],
                    );
                    renderer.blank(out);
                    renderer.status(out, Status::Success, "serving the production build");
                });
            }
            Event::Log { level, message } => render_log(ui, level, &message),
            // `page-failed` is emitted from exactly one place — `build()`'s
            // prerender loop in `driver.js` — so neither of these servers can
            // produce it today. It is reported rather than ignored because the
            // day one of them renders on demand it will, and an event named
            // `page-failed` that a server silently swallowed is the shape of
            // bug this pair of commands exists to make impossible. The server
            // carries on: one route that could not be rendered is not a reason
            // to stop answering every other request, which is `uf build`'s
            // decision to make and not a running server's.
            Event::PageFailed { url, error } => {
                render_log(ui, LogLevel::Error, &format!("{url} failed to render"));
                let _ = render_error(ui, &root, &error);
            }
            Event::Error(error) => {
                let failure = render_error(ui, &root, &error);
                let _ = driver.finish(&banner);
                return Err(failure);
            }
            // Everything the *build* says. Named rather than swept up by a
            // `_` so that the next event added to the driver comes back here
            // as a compile error instead of as silence — which is exactly how
            // `main` stopped compiling once, see #366 and #367.
            // Neither server watches: `uf preview` and `uf start` serve a
            // build, so a module changing under them changes nothing they are
            // showing.
            Event::ConfigLoaded { .. }
            | Event::Phase { .. }
            | Event::Page { .. }
            | Event::SourceChanged
            | Event::Done { .. }
            | Event::Config { .. } => {}
        }
    }
    driver.finish(match which {
        Server::Preview => "the preview server",
        Server::Start => "the server",
    })
}

/// The driver's arguments.
///
/// `--out-dir` is always passed, so the server reads the same directory the
/// build wrote rather than re-deriving a default: a project with
/// `build.outDir: "dist/docs"` would otherwise be served an empty `dist/`.
///
/// There is no `--strict-port` here, and its absence means opposite things on
/// the two servers, which is why it is not simply forwarded. `uf start` binds
/// what it is given or fails, always. `uf preview` is Vite's, and Vite moves to
/// the next free port unless told not to — so a typed port is a strict one, the
/// rule `uf dev` already follows for the reason recorded in
/// ubugeeei-prod/uf#234: a server up on a port nobody asked about is a server
/// that passes a test it is not running.
fn driver_args(host: Option<&str>, port: Option<u16>, out_dir: &str) -> Vec<String> {
    let mut args = vec![String::from("--out-dir"), out_dir.to_owned()];
    if let Some(bind) = host {
        args.push(String::from("--host"));
        args.push(bind.to_owned());
    }
    if let Some(port) = port {
        args.push(String::from("--port"));
        args.push(port.to_string());
        args.push(String::from("--strict-port"));
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_output_directory_is_always_named() {
        // A project whose build went to `dist/docs` and whose server read
        // `dist/` would serve an empty directory and 404 every page it had
        // just prerendered.
        assert_eq!(
            driver_args(None, None, "dist/docs"),
            vec![String::from("--out-dir"), String::from("dist/docs")]
        );
    }

    #[test]
    fn a_port_that_was_typed_is_the_port_the_server_binds() {
        assert_eq!(
            driver_args(None, Some(4173), "dist"),
            vec![
                String::from("--out-dir"),
                String::from("dist"),
                String::from("--port"),
                String::from("4173"),
                String::from("--strict-port"),
            ]
        );
    }

    #[test]
    fn a_host_is_forwarded_as_given() {
        assert_eq!(
            driver_args(Some("0.0.0.0"), None, "dist"),
            vec![
                String::from("--out-dir"),
                String::from("dist"),
                String::from("--host"),
                String::from("0.0.0.0"),
            ]
        );
    }
}
