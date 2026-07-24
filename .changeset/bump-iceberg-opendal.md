---
default: patch
---

# Bump Iceberg to 0.10 and pin opendal to 0.57

Update the Iceberg stack (`iceberg`, `iceberg-catalog-rest`,
`iceberg-storage-opendal`) to 0.10, and move `opendal` off the yanked 0.58.0 to
0.57 so the crate builds from a fresh checkout again. This dedupes opendal to a
single version in the dependency tree.
