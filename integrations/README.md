# CI integrations

Setup for the three CI systems, so that a pipeline installs `uf` in one step
instead of building it from source.

| System | File | How a project uses it |
| --- | --- | --- |
| GitHub Actions | [`github-actions/action.yml`](github-actions/action.yml) | `uses: ubugeeei-prod/uf/integrations/github-actions@uf@0.0.0-alpha.10` |
| GitLab CI | [`gitlab/uf.gitlab-ci.yml`](gitlab/uf.gitlab-ci.yml) | `include: [{ remote: "…/integrations/gitlab/uf.gitlab-ci.yml" }]`, then `extends: .uf` |
| CircleCI | [`circleci/orb.yml`](circleci/orb.yml) | `orbs: { uf: uniflowed/uf@1 }`, then `uf/install` or the `uf/run` job |

Each file documents its own inputs. What follows is what they have in common,
and why they are three thin files rather than three implementations.

## All three run the same installer

`https://setup.uniflowed.dev` serves
[`infra/cloudflare/setup-assets/install.sh`](../infra/cloudflare/setup-assets/install.sh),
which is what a human runs:

```sh
curl -fsSL https://setup.uniflowed.dev | sh
```

It resolves a version, downloads `uf-<target>.tar.gz` from GitHub Releases,
**verifies its sha256 against the checksum published beside it**, refuses an
archive whose members would land outside their own directory, unpacks it under
`$UF_INSTALL_ROOT/runtimes/uf@<version>`, and links `uf`, `ufr` and `ufx` into
`$UF_BIN_DIR`. Every one of those steps is a reason not to reimplement it three
times: an integration that fetched a tarball itself would be an integration that
skipped the checksum.

So each file here is the same four decisions in that system's own vocabulary:

1. where to install (`UF_INSTALL_ROOT`, `UF_BIN_DIR`), chosen so that the
   system's own cache can carry it;
2. how to get that directory onto `PATH` for the steps that follow;
3. when the cache may be believed;
4. what happens on a platform with no build.

`tools/ci/integrations-agree.sh` checks the first of those against the installer
itself: an integration may only set environment variables `install.sh` actually
reads, so a renamed variable fails here rather than in somebody's pipeline.

## Pin the version

Every file defaults to `latest` and every file tells you not to leave it there.
`latest` is *the newest release including prereleases* — which is every release
uf has published so far — so it moves under a pipeline that did not change.

It is also the one value the caches cannot help with. A cache keyed on the word
`latest` answers with whatever release was newest the first time the pipeline
ran, and goes on answering with it after a newer one ships; that is worse than
no cache, so all three skip the cache when the version is `latest`, and all
three say so where the option is documented.

A restored cache is not believed on the strength of the key either. Each
integration runs `uf --version` out of the restored directory first: a cache
that lost a symlink, or one restored onto a different architecture under a key
that did not distinguish it, looks like a hit and can run nothing.

## Platforms

uf publishes macOS and Linux binaries for `x86_64` and `aarch64`. There is no
Windows build, and none of these pretend otherwise:

* the GitHub action **fails** on a Windows runner with the reason, rather than
  skipping — a matrix leg that quietly does nothing still reports green;
* the GitLab template and the CircleCI orb default to a glibc image, and both
  say why Alpine is not offered: uf ships a `*-unknown-linux-gnu` binary, and a
  glibc binary does not start on musl.

On Windows, run uf under WSL2.

## Publishing the CircleCI orb

`circleci/orb.yml` is the orb *source*. Making it available as
`uniflowed/uf@1` needs a registered namespace and a publish, which is a release
step rather than something in this repository's CI:

```sh
circleci orb validate integrations/circleci/orb.yml
circleci orb publish integrations/circleci/orb.yml uniflowed/uf@<version>
```

Until it is published, a CircleCI config can use the file directly by copying
the `install` command's steps, or by vendoring the orb with
`circleci config pack`.
