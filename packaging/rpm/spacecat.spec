# Build offline from vendored crates (see scripts/make-rpm-sources.sh), so no
# network is needed here -- this builds cleanly in mock/COPR.
#
# Rust release binaries carry no DWARF here (the default release profile sets no
# debug), so there is nothing for find-debuginfo to harvest.
%global debug_package %{nil}

Name:           spacecat
Version:        0.2.1
Release:        1%{?dist}
Summary:        SpaceCat - Astronomical Observation System

License:        Apache-2.0
URL:            https://github.com/theatrus/spacecat

# Both sources are produced by scripts/make-rpm-sources.sh (the git archive does
# not contain the vendored crates needed for an offline build).
Source0:        %{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.xz

# edition = "2024" needs Rust >= 1.85; Fedora 43/44 ship newer.
BuildRequires:  rust >= 1.85.0
BuildRequires:  cargo
# libsqlite3-sys is built with the `bundled` feature (compiles SQLite from C),
# and aws-lc-sys (rustls crypto backend) builds a C library via CMake.
BuildRequires:  gcc
BuildRequires:  gcc-c++
BuildRequires:  cmake
# ring/aws-lc-sys invoke perl while assembling their crypto object files.
BuildRequires:  perl-interpreter
# systemd unit + sysusers.d handling (%%_unitdir, %%systemd_* macros, etc.).
BuildRequires:  systemd-rpm-macros

# Pulls the right Requires for the sysusers.d-created system user.
%{?sysusers_requires_compat}
Requires(post):   systemd
Requires(preun):  systemd
Requires(postun): systemd

# Runtime dependencies are resolved automatically by RPM's ELF dependency
# generator. SQLite is statically bundled and TLS uses rustls (no OpenSSL), so
# neither is a runtime dependency.

%description
SpaceCat is a Rust-based astronomical observation system that interfaces with
the NINA Advanced API for monitoring and posting to multiple chat services
(Discord, Matrix). It provides real-time event tracking, image history
management, autofocus analysis, and sequence automation across multiple
telescopes.

%prep
%autosetup -n %{name}-%{version} -p1
# rust-toolchain.toml pins a rustup channel; Fedora's cargo ignores it, but
# remove it so no rustup-based environment tries to fetch a toolchain offline.
rm -f rust-toolchain.toml
# Drop the vendored crates in beside the source and point cargo at them.
tar -xf %{SOURCE1}
mkdir -p .cargo
cat > .cargo/config.toml <<'EOF'
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
EOF

%build
# Crates resolve from ./vendor, so build offline.
export CARGO_NET_OFFLINE=true
cargo build --release --locked

%install
install -Dpm0755 target/release/%{name} %{buildroot}%{_bindir}/%{name}

# systemd chat-updater service integration.
install -Dpm0644 packaging/rpm/systemd/spacecat.service \
    %{buildroot}%{_unitdir}/%{name}.service
install -Dpm0644 packaging/rpm/systemd/spacecat.sysusers \
    %{buildroot}%{_sysusersdir}/%{name}.conf
install -Dpm0644 packaging/rpm/systemd/spacecat.conf \
    %{buildroot}%{_sysconfdir}/%{name}/spacecat.conf
install -Dpm0640 packaging/rpm/spacecat-default.json \
    %{buildroot}%{_sysconfdir}/%{name}/config.json

# The spacecat system user is created by the sysusers.d file trigger that
# systemd installs for %%{_sysusersdir}, so no %%pre useradd is needed.
%post
%systemd_post %{name}.service

%preun
%systemd_preun %{name}.service

%postun
%systemd_postun_with_restart %{name}.service

%files
%license LICENSE
%doc README.md config.example.json
%{_bindir}/%{name}
%{_unitdir}/%{name}.service
%{_sysusersdir}/%{name}.conf
%dir %{_sysconfdir}/%{name}
%config(noreplace) %{_sysconfdir}/%{name}/spacecat.conf
# config.json holds chat credentials; keep it group-readable by the service
# user only and never clobber operator edits on upgrade.
%attr(0640,root,spacecat) %config(noreplace) %{_sysconfdir}/%{name}/config.json

%changelog
* Thu Jul 02 2026 Yann Ramin <github@theatr.us> - 0.2.1-1
- Fix `cargo fmt` violations that broke CI linting on main

* Thu Jul 02 2026 Yann Ramin <github@theatr.us> - 0.2.0-1
- Add exponential-backoff reconnect logic for offline telescopes, with
  configurable per-telescope thresholds and debounced chat alerts on
  offline/reconnect transitions

* Mon Jun 30 2026 Yann Ramin <github@theatr.us> - 0.1.0-1
- Rework packaging for offline mock/COPR builds from vendored crates
- Create the spacecat system user via sysusers.d
- Ship spacecat.service (chat-updater mode) with an EnvironmentFile of knobs
  and a disabled-by-default /etc/spacecat/config.json
