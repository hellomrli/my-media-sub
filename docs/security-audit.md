# Dependency audit policy

CI runs RustSec on every push. `RUSTSEC-2023-0071` is temporarily ignored because the upstream `rsa` 0.9 line has no patched release. It is transitive through web-push and my-media-sub does not expose an RSA private-key operation to remote callers. The exception must be removed when upstream ships a constant-time replacement.

`RUSTSEC-2026-0185` is fixed by pinning `quinn-proto >= 0.11.15` in `Cargo.lock`.
