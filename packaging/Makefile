# SpaceCat RPM Packaging Makefile

PACKAGE_NAME := spacecat
VERSION := $(shell grep '^version = ' ../Cargo.toml | sed 's/version = "\(.*\)"/\1/')
RPMBUILD_DIR := $(HOME)/rpmbuild

.PHONY: all clean rpm install-deps check-deps build-binary

all: rpm

# Check if required tools are installed
check-deps:
	@command -v rpmbuild >/dev/null 2>&1 || { echo "rpmbuild is required but not installed. Run 'make install-deps'" >&2; exit 1; }
	@command -v cargo >/dev/null 2>&1 || { echo "cargo is required but not installed. Run 'make install-deps'" >&2; exit 1; }

# Install build dependencies (Fedora)
install-deps:
	sudo dnf install -y rpm-build rpm-devel rust cargo systemd-rpm-macros openssl-devel

# Build the binary for packaging
build-binary:
	cd .. && cargo build --release

# Build RPM package
rpm: check-deps build-binary
	@echo "Building RPM for $(PACKAGE_NAME) version $(VERSION)"
	./build-rpm.sh

# Install the RPM (requires sudo)
install: rpm
	@RPM_FILE=$$(find $(RPMBUILD_DIR)/RPMS -name "$(PACKAGE_NAME)-$(VERSION)-*.rpm" -type f | head -n1); \
	if [ -n "$$RPM_FILE" ]; then \
		echo "Installing $$RPM_FILE"; \
		sudo dnf install -y "$$RPM_FILE"; \
	else \
		echo "RPM file not found. Build may have failed." >&2; \
		exit 1; \
	fi

# Enable and start the service
enable-service:
	sudo systemctl enable --now $(PACKAGE_NAME).service
	@echo "Service enabled and started. Check status with: systemctl status $(PACKAGE_NAME).service"

# Check service status
status:
	sudo systemctl status $(PACKAGE_NAME).service

# View service logs
logs:
	journalctl -u $(PACKAGE_NAME).service -f

# Clean build artifacts
clean:
	rm -rf $(RPMBUILD_DIR)/BUILD/$(PACKAGE_NAME)-*
	rm -rf $(RPMBUILD_DIR)/BUILDROOT/$(PACKAGE_NAME)-*
	rm -f $(RPMBUILD_DIR)/SOURCES/$(PACKAGE_NAME)-*.tar.gz
	rm -f $(RPMBUILD_DIR)/SPECS/$(PACKAGE_NAME).spec
	rm -f $(RPMBUILD_DIR)/SRPMS/$(PACKAGE_NAME)-*.src.rpm
	rm -f $(RPMBUILD_DIR)/RPMS/*/$(PACKAGE_NAME)-*.rpm

# Show help
help:
	@echo "SpaceCat RPM Packaging"
	@echo ""
	@echo "Available targets:"
	@echo "  all            - Build RPM package (default)"
	@echo "  check-deps     - Check if build dependencies are installed"
	@echo "  install-deps   - Install build dependencies (Fedora)"
	@echo "  build-binary   - Build the Rust binary"
	@echo "  rpm            - Build RPM package"
	@echo "  install        - Install the built RPM package"
	@echo "  enable-service - Enable and start the systemd service"
	@echo "  status         - Check service status"
	@echo "  logs           - View service logs"
	@echo "  clean          - Clean build artifacts"
	@echo "  help           - Show this help message"
	@echo ""
	@echo "Quick start:"
	@echo "  make install-deps  # Install build tools"
	@echo "  make rpm           # Build RPM"
	@echo "  make install       # Install RPM"
	@echo "  make enable-service # Start service"
