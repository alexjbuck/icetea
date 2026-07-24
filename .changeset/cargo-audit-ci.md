---
default: patch
---

# Speed up and fix the cargo-audit CI workflow

Use a prebuilt cargo-audit binary and Node 24 actions, and fix the subcommand
shim invocation in the security-audit workflow.
