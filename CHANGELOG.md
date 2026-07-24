# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
## 0.2.2 (2026-07-24)

### Fixes

- Refresh README documentation

#### Bump Iceberg to 0.10 and pin opendal to 0.57

Update the Iceberg stack (`iceberg`, `iceberg-catalog-rest`,
`iceberg-storage-opendal`) to 0.10, and move `opendal` off the yanked 0.58.0 to
0.57 so the crate builds from a fresh checkout again. This dedupes opendal to a
single version in the dependency tree.

#### Speed up and fix the cargo-audit CI workflow

Use a prebuilt cargo-audit binary and Node 24 actions, and fix the subcommand
shim invocation in the security-audit workflow.

## 0.2.1 (2026-07-17)

### Fixes

- Added changesets and versioning and releases!
- Bump Iceberg/DataFusion dependencies and set MSRV to 1.94
