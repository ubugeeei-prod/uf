# Cloudflare distribution infrastructure

This directory is the IaC source of truth for `uniflowed.dev`.

## Topology

- `uniflowed.dev` and `www.uniflowed.dev`: redirect to docs.
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
