# Chatstronomy integration contracts

This directory contains the versioned wire contracts consumed by external
integrations such as the Chatstronomy N.I.N.A. plugin.

- `direct/v1/` defines the JSON messages exchanged with a centralized hub.
- `runtime-manifest-v1.schema.json` defines the immutable release manifest used
  to locate and verify the bundled Windows runtime.

Published releases attach these files as `chatstronomy-plugin-contracts-v1.zip`.
Consumers must pin a release tag and SHA-256 values; they must not resolve the
latest release at build or install time.
