# Cloudflare distribution infrastructure

This directory is the IaC source of truth for `uniflowed.dev`.

## Topology

- `uniflowed.dev`: the landing page, from `workers/root.js`. It answers `/`,
  `/robots.txt` and `/brand/*` — the last out of the repository's own `brand/`
  directory, bound as `ASSETS` and read in place rather than copied — and
  **308s every other path to `docs.uniflowed.dev`**, which is the whole of what
  this worker used to do. That fallthrough is the compatibility rule: links to
  `uniflowed.dev/guide`, `/og.png` and `/sitemap.xml` exist in issues and in
  other people's pages, and none of them may stop resolving.
  `tools/ci/apex-routes.sh` drives the handler and fails when one does.
- `www.uniflowed.dev`: 308 to the apex, same path. One origin is canonical;
  two hostnames serving one document split the cache and need a `rel=canonical`
  afterwards to say which of them counts.
- `docs.uniflowed.dev`: Workers Static Assets for the generated docs site.
- `setup.uniflowed.dev`: Worker endpoint for `curl -fsSL https://setup.uniflowed.dev | sh`.
- `releases.uniflowed.dev`: public R2 custom domain for release archives.
- `cache.uniflowed.dev`: optional public R2 custom domain reserved for Nix binary cache objects.

## Apply

Use OpenTofu or Terraform. The Cloudflare provider reads `CLOUDFLARE_API_TOKEN`
from the environment.

Enable R2 for the Cloudflare account in the dashboard before applying. If R2 is
not enabled yet, Cloudflare returns `10042: Please enable R2 through the
Cloudflare Dashboard` while creating the release buckets.

```sh
tools/docs/build.sh
export CLOUDFLARE_API_TOKEN=...
tofu -chdir=infra/cloudflare init
tofu -chdir=infra/cloudflare apply -var account_id=... -var zone_id=...
```

Terraform works with the same configuration:

```sh
terraform -chdir=infra/cloudflare init
terraform -chdir=infra/cloudflare apply -var account_id=... -var zone_id=...
```

The API token needs enough permissions to manage Workers, Workers custom
domains, DNS records in `uniflowed.dev`, and R2 buckets/custom domains.

## Docs Deploy

The generated docs site is dogfooded through `uf build`. The same build script
copies `brand/` into `docs/dist/docs/brand` before the bundle report is written.

```sh
tools/docs/build.sh
npx --yes wrangler@4.128.0 deploy --dry-run --config infra/cloudflare/wrangler.docs.jsonc
npx --yes wrangler@4.128.0 deploy --config infra/cloudflare/wrangler.docs.jsonc
npx --yes wrangler@4.128.0 deploy --dry-run --config infra/cloudflare/wrangler.setup.jsonc
npx --yes wrangler@4.128.0 deploy --config infra/cloudflare/wrangler.setup.jsonc
```

`.github/workflows/docs.yml` builds docs for pull requests and deploys the
existing `uf-docs` and `uf-setup` Workers from `main` when
`CLOUDFLARE_ACCOUNT_ID` and `CLOUDFLARE_API_TOKEN` are available.

## The apex

`uf-root` is **not** in that deploy job, and that is deliberate: it is the one
Worker whose script `main.tf` owns, and two systems writing one resource is how
a `tofu apply` months later silently reverts a landing page. Pick one. Terraform
is the default because the apex and `www` custom domains are declared there too:

```sh
tofu -chdir=infra/cloudflare apply -var account_id=... -var zone_id=...
```

Wrangler deploys the same script and assets without touching DNS, which is the
faster loop while editing the page:

```sh
npx --yes wrangler@4.128.0 deploy --dry-run --config infra/cloudflare/wrangler.root.jsonc
npx --yes wrangler@4.128.0 deploy --config infra/cloudflare/wrangler.root.jsonc
```

Locally, `wrangler dev` serves the real thing — the page, the `brand/` assets
and every redirect — with no account and no credentials:

```sh
npx --yes wrangler@4.128.0 dev --config infra/cloudflare/wrangler.root.jsonc
```

What it answers is checked rather than described. `uf run apex:routes` imports
the worker, calls its `fetch` with an `ASSETS` binding over `brand/`, and fails
when `/` stops being a page, when a path that used to redirect stops resolving
at the same path, when `www` points anywhere but the apex, when the page loads
something this worker cannot serve, or when the `style-src` hash no longer
matches the stylesheet the page returned. `uf run apex:routes:test` checks that
that check still catches each of those.

## Release Upload

`.github/workflows/release.yml` builds every target on `uf@*` tags, verifies the
installer against each archive, and publishes to **GitHub Releases**, which is
what `install.sh` downloads from by default. That path needs no Cloudflare
secrets, so an unconfigured account cannot produce a release that 404s.

R2 is an optional mirror. The same workflow uploads there afterwards when both
secrets exist, and skips with a notice when they do not:

- `CLOUDFLARE_API_TOKEN`
- `CLOUDFLARE_ACCOUNT_ID`

Setting `UF_RELEASE_BASE=https://releases.uniflowed.dev/uf` points the installer
at the mirror, which serves this object layout in the `uf-releases` bucket:

```txt
uf/latest/VERSION
uf/latest/uf-<target>.tar.gz
uf/latest/uf-<target>.tar.gz.sha256
uf/<version>/VERSION
uf/<version>/uf-<target>.tar.gz
uf/<version>/uf-<target>.tar.gz.sha256
```

Upload manually from a built release directory:

```sh
UF_R2_DRY_RUN=1 tools/release/publish-r2.sh dist/release/uf/0.1.0 --latest
tools/release/publish-r2.sh dist/release/uf/0.1.0 --latest
```

## Verifying the installer

`tools/release/test-install.sh` packages a release, serves it over HTTP, and
runs the real `install.sh` against it — covering `latest` resolution, pinned
`uf@<version>` installs, reinstalls, a tampered archive, an archive whose
members escape the extraction root, and a version that does not exist. CI runs
it on every pull request, and the release workflow runs it per target before
anything is published.

```sh
tools/release/test-install.sh
```
