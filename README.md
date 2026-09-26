<div align="center">

<img width="200" src="brand/uniflowed-mark.svg" alt="">

# uf

**A toolchain for React and Modern Flow.**

[Getting started](https://docs.uniflowed.dev/guide/start) ·
[Documentation](https://docs.uniflowed.dev) ·
[Reference](https://docs.uniflowed.dev/reference) ·
[Releases](https://github.com/ubugeeei-prod/uf/releases)

</div>

uf provides development builds, production builds, tests, formatting, linting
and type checking through one native CLI and one `uf.config.js`.
It uses the official Flow parser, React Compiler and Vite.

uf is at `0.x`, and that still means commands, configuration and package APIs
may change between releases; a minor release (`0.1.0` to `0.2.0`) may break
them, and its notes say how. See the [release notes](https://github.com/ubugeeei-prod/uf/releases)
and [current capabilities](https://docs.uniflowed.dev/guide/scope).

## Install

On macOS or Linux:

```sh
curl -fsSL https://setup.uniflowed.dev | sh
```

On Windows, use PowerShell:

```powershell
irm https://setup.uniflowed.dev/install.ps1 | iex
```

You also need a JavaScript host such as Node.js. The
[installation guide](https://docs.uniflowed.dev/guide/install) covers supported
hosts, download verification, PATH setup, Nix and building from source.

## Create a project

```sh
uf new my-app
cd my-app
uf install
uf dev
```

Run `uf test` to test the project and `uf build` to create a production build.
[Your first project](https://docs.uniflowed.dev/guide/project) explains the files
and common commands. [Build a reading list](https://docs.uniflowed.dev/guide/tutorial)
walks through routes, shared state, forms and tests.

## Documentation

- [Getting started](https://docs.uniflowed.dev/guide/start): installation, projects and editor setup.
- [Build an app](https://docs.uniflowed.dev/guide/build-an-app): routing, data, state and UI.
- [Reference](https://docs.uniflowed.dev/reference): commands, configuration and package APIs.
- [Migrating to uf](https://docs.uniflowed.dev/guide/migrate): use uf with an existing application.

## Examples

- [SPA task board](examples/spa): client rendering, shared state and static hosting.
- [GraphQL SNS](examples/simple-sns-graphql): Relay and server data.
- [Native SNS](examples/simple-sns-native): React Native.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for local setup and checks,
[the architecture](docs/architecture.md) for the implementation, and
[the roadmap](docs/roadmap.md) for planned work.

## License

[MIT](LICENSE)
