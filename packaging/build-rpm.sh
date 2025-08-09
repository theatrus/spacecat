#!/bin/bash
set -e

# SpaceCat RPM Build Script
# This script builds an RPM package for Fedora/RHEL/CentOS

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
PACKAGE_NAME="spacecat"
VERSION=$(grep '^version = ' "$PROJECT_DIR/Cargo.toml" | sed 's/version = "\(.*\)"/\1/')

echo "Building RPM for $PACKAGE_NAME version $VERSION"

# Create RPM build environment
RPMBUILD_DIR="$HOME/rpmbuild"
mkdir -p "$RPMBUILD_DIR"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}

# Copy spec file
cp "$SCRIPT_DIR/rpm/spacecat.spec" "$RPMBUILD_DIR/SPECS/"

# Create source tarball
TARBALL_NAME="$PACKAGE_NAME-$VERSION.tar.gz"
TEMP_DIR=$(mktemp -d)
SOURCE_DIR="$TEMP_DIR/$PACKAGE_NAME-$VERSION"

echo "Creating source tarball..."
mkdir -p "$SOURCE_DIR"
cp -r "$PROJECT_DIR"/* "$SOURCE_DIR/" 2>/dev/null || true

# Exclude build artifacts and development files
rm -rf "$SOURCE_DIR/target"
rm -rf "$SOURCE_DIR/.git"
rm -rf "$SOURCE_DIR/.github"
rm -f "$SOURCE_DIR/.gitignore"
rm -f "$SOURCE_DIR"/*.jpg "$SOURCE_DIR"/*.png

cd "$TEMP_DIR"
tar -czf "$RPMBUILD_DIR/SOURCES/$TARBALL_NAME" "$PACKAGE_NAME-$VERSION"
cd - > /dev/null

# Clean up temp directory
rm -rf "$TEMP_DIR"

# Build RPM
echo "Building RPM package..."
rpmbuild -ba "$RPMBUILD_DIR/SPECS/spacecat.spec"

echo ""
echo "RPM build completed!"
echo "Packages created:"
echo "  Source RPM: $(find "$RPMBUILD_DIR/SRPMS" -name "*.src.rpm" -type f)"
echo "  Binary RPM: $(find "$RPMBUILD_DIR/RPMS" -name "*.rpm" -type f)"

echo ""
echo "To install the RPM:"
echo "  sudo dnf install $RPMBUILD_DIR/RPMS/x86_64/$PACKAGE_NAME-$VERSION-1.*.x86_64.rpm"
echo ""
echo "To start the service:"
echo "  sudo systemctl enable --now spacecat.service"
echo ""
echo "To check service status:"
echo "  sudo systemctl status spacecat.service"
echo "  journalctl -u spacecat.service -f"