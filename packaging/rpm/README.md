# Fedora / RPM packaging

This directory packages SpaceCat as a native RPM that builds with mainline RPM
tooling (`rpmbuild`, `mock`, COPR) on Fedora 43, Fedora 44, and other recent
Fedora releases.

## Design

SpaceCat pulls a large Cargo dependency tree (Discord, Matrix, bundled SQLite,
rustls). A Fedora build environment (mock/COPR) is **offline**, so the one
network step -- fetching those crates -- is done once, up front, by
`scripts/make-rpm-sources.sh`:

1. Export a clean tree from git.
2. `cargo vendor` every crate into `vendor/`. The spec writes a
   `.cargo/config.toml` pointing at it and builds with `CARGO_NET_OFFLINE=true`.

The result is two sources:

| Source  | File                             | Contents                     |
| ------- | -------------------------------- | ---------------------------- |
| Source0 | `spacecat-<ver>.tar.gz`          | source tree                  |
| Source1 | `spacecat-<ver>-vendor.tar.xz`   | vendored crates (`./vendor`) |

SQLite is statically bundled and TLS uses rustls, so the package has no OpenSSL
or system SQLite runtime dependency; RPM's ELF dependency generator resolves the
rest automatically.

## Systemd chat-updater service

The package ships a `spacecat.service` unit that runs `spacecat chat-updater`
under a dedicated, unprivileged `spacecat` system user (created via
`sysusers.d`). It is **not** enabled by default, and won't start until you fill
in a config.

| Path                          | Purpose                                             |
| ----------------------------- | --------------------------------------------------- |
| `/etc/spacecat/config.json`   | application config (telescopes, chat services)      |
| `/etc/spacecat/spacecat.conf` | `EnvironmentFile` -- poll interval, extra CLI args  |

```bash
# Configure telescopes + chat services, then enable + start:
sudoedit /etc/spacecat/config.json
sudo systemctl enable --now spacecat

# Watch it run:
journalctl -u spacecat -f
```

`config.json` ships disabled (`enabled: false` for both chat services). The unit
is sandboxed (`ProtectSystem=strict`, `ProtectHome=yes`, restricted
syscalls/capabilities) and only needs outbound network access to the NINA API
and the chat services.

## Build locally

```bash
# Install tooling (Fedora):
sudo dnf install -y rpm-build rpmdevtools cargo rust gcc gcc-c++ cmake git
rpmdev-setuptree

# Generate the two source tarballs (needs network: cargo vendor).
./scripts/make-rpm-sources.sh                    # -> ~/rpmbuild/SOURCES

# Build (offline from here on).
rpmbuild -ba packaging/rpm/spacecat.spec

# RPMs land in ~/rpmbuild/RPMS/<arch>/
```

## Build in clean Fedora containers (podman)

`build-in-podman.sh` reproduces the CI build locally: it runs the whole flow
(toolchain install, source generation, `rpmbuild`, and an `rpm -i` +
`spacecat --help` smoke test) inside throwaway `fedora:<ver>` containers and
drops the artifacts on the host.

```bash
# Builds Fedora 43 and 44 in parallel into ./dist/rpm/fedora-<ver>/
./packaging/rpm/build-in-podman.sh

# Pick releases and an output directory; build one at a time:
./packaging/rpm/build-in-podman.sh 43 44 --outdir /tmp/spacecat-rpms --sequential
```

Each release lands in `<outdir>/fedora-<ver>/` alongside a `build.log`. Only
podman is required on the host (the container does the rest; network is needed
for cargo vendor).

## Build in mock (clean chroot, e.g. Fedora 44)

```bash
./scripts/make-rpm-sources.sh --outdir /tmp/spacecat-sources
rpmbuild -bs packaging/rpm/spacecat.spec \
    --define "_sourcedir /tmp/spacecat-sources"
mock -r fedora-44-x86_64 ~/rpmbuild/SRPMS/spacecat-*.src.rpm
```

## Releasing a new version

1. Bump `Version:` in `spacecat.spec` to match `Cargo.toml`.
2. Add a `%changelog` entry.
3. Regenerate sources for the tag: `./scripts/make-rpm-sources.sh --ref vX.Y.Z`.

CI (`.github/workflows/rpm.yml`) builds the RPMs in Fedora 43 and 44 containers
on every push and pull request, uploads them as artifacts, and attaches the
binary RPMs to the GitHub release on tag builds.
