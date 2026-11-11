# Distribution and publishing

The project is prepared for the repository `fcenedes/mcp-equidia-rs`, the GHCR
image `ghcr.io/fcenedes/mcp-equidia-rs`, and the crate `mcp-equidia-rs`.
These are intended publication locations. Preparing this repository does not
make the image or crate publicly available; a first publication is still needed.
Crate-name availability must be confirmed with crates.io before that publication.

## Local artifacts

The prepared Docker image is tagged `mcp-equidia-rs:0.1.0`. The release build can
also be exported to a portable OCI archive supporting both Linux architectures:

```sh
docker buildx create --name equidia-release --driver docker-container
docker buildx build --builder equidia-release --platform linux/amd64,linux/arm64 \
  --build-arg VERSION=0.1.0 --tag mcp-equidia-rs:0.1.0 \
  --output type=oci,dest=dist/mcp-equidia-rs-0.1.0-oci.tar .
docker load --input dist/mcp-equidia-rs-0.1.0-oci.tar
docker run --rm -i mcp-equidia-rs:0.1.0 serve stdio
```

Loading a multi-platform OCI archive requires an engine with the containerd image
store; the archive can also be imported into other OCI-compatible tooling. For
engines using the classic image store, use a single-platform Docker archive or
pull the matching platform from GHCR after publishing.

For HTTP, the MCP listener token protects the local service. It is never sent
to Equidia, and no Equidia token is required:

```sh
docker run --rm --read-only --cap-drop ALL --security-opt no-new-privileges \
  -p 127.0.0.1:3000:3000 -e MCP_HTTP_TOKEN='choose-a-listener-token' \
  mcp-equidia-rs:0.1.0
```

Package a local executable with its documentation and checksum:

```sh
cargo build --release --locked
python3 scripts/release.py package --target aarch64-apple-darwin \
  --binary target/release/mcp-equidia-rs
python3 scripts/release.py checksums
```

The archive name records the platform; use the target matching the binary you
built. CI builds and runs each executable natively before packaging it.

## GitHub and GHCR automation

The release workflow runs on a `v*` tag, or manually for an existing tag. It:

1. Requires the tag to match `Cargo.toml` exactly.
2. Runs the reusable CI checks: Rust tests, lint, MSRV, coverage, crate archive
   audit, docs.rs-style documentation build and Python release-helper tests.
3. Builds and smoke-tests four native executables.
4. Builds both Docker architectures natively, tests MCP stdio and HTTP, and
   pushes the tested architecture images to GHCR.
5. Verifies all executable checksums, assembles the multi-platform image from
   the architecture digests, and publishes the GitHub release with its assets.

| Artifact | Platforms |
| --- | --- |
| Docker image | `linux/amd64`, `linux/arm64` |
| Linux executable archives | `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` |
| macOS executable archives | `aarch64-apple-darwin`, `x86_64-apple-darwin` |

Linux executables are built on Ubuntu 22.04 (glibc 2.35). macOS executables are
unsigned and are not notarized. Containers use Debian Bookworm and run as
UID/GID 10001 with CA certificates and no application secrets baked in.

A stable `v0.1.0` release publishes these image tags:

```text
ghcr.io/fcenedes/mcp-equidia-rs:0.1.0
ghcr.io/fcenedes/mcp-equidia-rs:0.1
ghcr.io/fcenedes/mcp-equidia-rs:latest
```

Architecture staging tags use `0.1.0-amd64` and `0.1.0-arm64`. Prereleases only
receive their exact version tag. Version tags must be treated as immutable;
make a new version instead of moving an existing release tag.

GHCR uses the workflow's `GITHUB_TOKEN` with `packages: write`; it does not need
a personal registry token. The first package may need its visibility set to
public in GitHub Packages before anonymous pulls work. See the official
[GitHub container publishing guide](https://docs.github.com/en/actions/tutorials/publish-packages/publish-docker-images).

After publishing, use the image-based Compose file:

```sh
MCP_HTTP_TOKEN='choose-a-listener-token' \
  docker compose -f deploy/compose.image.yaml up -d
```

Override `EQUIDIA_IMAGE` with an exact version or digest to pin a deployment.
The original `deploy/compose.yaml` continues to build from local source.

## First repository release

Once the files have been reviewed and publication is desired:

```sh
git add .
git commit -m "Initial read-only Equidia MCP server"
gh repo create fcenedes/mcp-equidia-rs --public --source . --remote origin --push
git tag v0.1.0
git push origin v0.1.0
```

Enable Actions and permit its explicit package-write permission. All third-party
workflow actions are pinned to commit SHAs, with Dependabot updates configured.
CI tests container transports without live Equidia calls; live API probes remain
explicit so an upstream outage cannot block packaging.

## crates.io and docs.rs

The crate manifest has repository, homepage, documentation, license and category
metadata. Its explicit package include list contains the source, API snapshot,
generated catalogue, access policy, tests, scripts, documentation, deployment
examples and licenses. The package audit rejects unexpected paths, symlinks,
credential filenames and nonempty example tokens.

The docs.rs settings build all features for `x86_64-unknown-linux-gnu` only.
Documentation uses bundled files and does not contact Equidia at build time.
docs.rs [automatically builds documentation after crates.io publication](https://docs.rs/about/builds);
no separate documentation upload is needed. See the supported
[docs.rs metadata](https://docs.rs/about/metadata).

The `Publish crate` workflow is manually dispatched for a published GitHub
release tag. Configure the `crates-io` GitHub environment and its
`CARGO_REGISTRY_TOKEN` secret using a crates.io publish token authorized for
this package. Environment review rules can be configured in GitHub if desired.

```sh
gh api --method PUT repos/fcenedes/mcp-equidia-rs/environments/crates-io
gh secret set CARGO_REGISTRY_TOKEN --env crates-io --repo fcenedes/mcp-equidia-rs
gh workflow run publish.yml --repo fcenedes/mcp-equidia-rs -f tag=v0.1.0
```

The workflow verifies the tag and published GitHub release, reruns tests,
builds and audits the actual `.crate` archive, verifies the docs.rs configuration
and then calls `cargo publish --locked`. Publishing a crate version is permanent;
the workflow does not trigger automatically on source pushes.

Before publication, reproduce the checks locally:

```sh
cargo package --locked --allow-dirty
python3 scripts/release.py audit-package
cargo publish --dry-run --locked --allow-dirty
DOCS_RS=1 RUSTDOCFLAGS='--cfg docsrs -D warnings' \
  cargo doc --locked --no-deps --lib --all-features
cargo test --doc --locked
python3 -m unittest discover -s tests -p 'test_*.py'
actionlint
```

`--allow-dirty` is useful for local preparation. Hosted publishing uses a clean
checkout of the version tag. Package verification rebuilds from the `.crate`
contents, catching missing source or documentation assets.

The publish dry run contacts crates.io even though it does not upload. A
registry outage or access restriction can block that step independently of a
successful local package verification.
