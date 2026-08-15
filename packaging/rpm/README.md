# Fedora / RPM packaging

This directory packages Chatstronomy as a native RPM that builds with mainline RPM
tooling (`rpmbuild`, `mock`, COPR) on Fedora 43, Fedora 44, and other recent
Fedora releases.

## Design

Chatstronomy pulls a large Cargo dependency tree (Discord, Matrix, bundled SQLite,
rustls). A Fedora build environment (mock/COPR) is **offline**, so the one
network step -- fetching those crates -- is done once, up front, by
`scripts/make-rpm-sources.sh`:

1. Export a clean tree from git.
2. `cargo vendor` every crate into `vendor/`. The spec writes a
   `.cargo/config.toml` pointing at it and builds with `CARGO_NET_OFFLINE=true`.

The result is two sources:

| Source  | File                             | Contents                     |
| ------- | -------------------------------- | ---------------------------- |
| Source0 | `chatstronomy-<ver>.tar.gz`          | source tree                  |
| Source1 | `chatstronomy-<ver>-vendor.tar.xz`   | vendored crates (`./vendor`) |

SQLite is statically bundled and TLS uses rustls, so the package has no OpenSSL
or system SQLite runtime dependency; RPM's ELF dependency generator resolves the
rest automatically.

## Systemd chat-updater service

The package ships a `chatstronomy.service` unit that runs `chatstronomy chat-updater`
under a dedicated, unprivileged `chatstronomy` system user (created via
`sysusers.d`). It is **not** enabled by default, and won't start until you fill
in a config.

| Path                          | Purpose                                             |
| ----------------------------- | --------------------------------------------------- |
| `/etc/chatstronomy/config.json`   | application config (telescopes, chat services)      |
| `/etc/chatstronomy/chatstronomy.conf` | `EnvironmentFile` -- poll interval, extra CLI args  |

```bash
# Configure telescopes + chat services, then enable + start:
sudoedit /etc/chatstronomy/config.json
sudo systemctl enable --now chatstronomy

# Watch it run:
journalctl -u chatstronomy -f
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
rpmbuild -ba packaging/rpm/chatstronomy.spec

# RPMs land in ~/rpmbuild/RPMS/<arch>/
```

## Build in clean Fedora containers (podman)

`build-in-podman.sh` reproduces the CI build locally: it runs the whole flow
(toolchain install, source generation, `rpmbuild`, and an `rpm -i` +
`chatstronomy --help` smoke test) inside throwaway `fedora:<ver>` containers and
drops the artifacts on the host.

```bash
# Builds Fedora 43 and 44 in parallel into ./dist/rpm/fedora-<ver>/
./packaging/rpm/build-in-podman.sh

# Pick releases and an output directory; build one at a time:
./packaging/rpm/build-in-podman.sh 43 44 --outdir /tmp/chatstronomy-rpms --sequential
```

Each release lands in `<outdir>/fedora-<ver>/` alongside a `build.log`. Only
podman is required on the host (the container does the rest; network is needed
for cargo vendor).

## Build in mock (clean chroot, e.g. Fedora 44)

```bash
./scripts/make-rpm-sources.sh --outdir /tmp/chatstronomy-sources
rpmbuild -bs packaging/rpm/chatstronomy.spec \
    --define "_sourcedir /tmp/chatstronomy-sources"
mock -r fedora-44-x86_64 ~/rpmbuild/SRPMS/chatstronomy-*.src.rpm
```

## Releasing a new version

1. Bump `Version:` in `chatstronomy.spec` to match `Cargo.toml`.
2. Add a `%changelog` entry.
3. Regenerate sources for the tag: `./scripts/make-rpm-sources.sh --ref vX.Y.Z`.

CI (`.github/workflows/rpm.yml`) builds the RPMs in Fedora 43 and 44 containers
on every push and pull request, uploads them as artifacts, and attaches the
binary RPMs to the GitHub release on tag builds.

## Upgrading a host that still has `spacecat` installed

The package was renamed. The spec carries `Obsoletes: spacecat < 0.3.1` and
`Provides: spacecat`, so `dnf install chatstronomy` replaces the old package
rather than installing beside it. Without those, dnf has no reason to connect
the two names and you end up with both — two units, two system users, two
config directories, and the old bot still posting to the same channel.

**The configuration does not migrate itself.** `/etc/spacecat/config.json` is
`%config(noreplace)`, so removing the old package leaves it as
`/etc/spacecat/config.json.rpmsave` and the new package installs its own
disabled default. Carry the credentials across by hand:

```bash
sudo cp /etc/spacecat/config.json.rpmsave /etc/chatstronomy/config.json
sudo chown root:chatstronomy /etc/chatstronomy/config.json
sudo chmod 0640 /etc/chatstronomy/config.json
sudo systemctl restart chatstronomy
```

Check the old unit is gone (`systemctl status spacecat` should report no such
unit) before assuming the rename completed — a leftover enabled `spacecat.service`
is the failure that looks like duplicate messages in your channel.
