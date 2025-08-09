Name:           spacecat
Version:        0.1.0
Release:        1%{?dist}
Summary:        SpaceCat - Astronomical Observation System

License:        Apache-2.0
URL:            https://github.com/theatrus/spacecat
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust >= 1.80.0
BuildRequires:  cargo
BuildRequires:  systemd-rpm-macros
Requires:       systemd
Requires:       openssl-libs

%description
SpaceCat is a Rust-based astronomical observation system that interfaces with 
SpaceCat API for telescope automation and monitoring. The system provides 
real-time event tracking, image history management, and sequence automation.

%prep
%autosetup

%build
# Build the binary using cargo
cargo build --release

%install
# Create directories
install -d %{buildroot}%{_bindir}
install -d %{buildroot}%{_sysconfdir}/spacecat
install -d %{buildroot}%{_unitdir}
install -d %{buildroot}%{_var}/log/spacecat

# Install binary
install -m 0755 target/release/spacecat %{buildroot}%{_bindir}/spacecat

# Install systemd service file
install -m 0644 packaging/systemd/spacecat.service %{buildroot}%{_unitdir}/spacecat.service

# Install default configuration
install -m 0644 packaging/config/spacecat.conf %{buildroot}%{_sysconfdir}/spacecat/config.json

%pre
# Create spacecat user and group
getent group spacecat >/dev/null || groupadd -r spacecat
getent passwd spacecat >/dev/null || \
    useradd -r -g spacecat -d /var/lib/spacecat -s /sbin/nologin \
    -c "SpaceCat Astronomy System" spacecat

%post
%systemd_post spacecat.service

%preun
%systemd_preun spacecat.service

%postun
%systemd_postun_with_restart spacecat.service
if [ $1 -eq 0 ] ; then
    # Package removal, not upgrade
    userdel spacecat >/dev/null 2>&1 || :
    groupdel spacecat >/dev/null 2>&1 || :
fi

%files
%license LICENSE
%doc README.md CLAUDE.md
%{_bindir}/spacecat
%{_unitdir}/spacecat.service
%config(noreplace) %{_sysconfdir}/spacecat/config.json
%attr(0755,spacecat,spacecat) %dir %{_var}/log/spacecat

%changelog
* Fri Aug 09 2025 Yann Ramin <github@theatr.us> - 0.1.0-1
- Initial RPM package
- Added systemd service for discord-updater mode
- Includes configuration file and logging directory
