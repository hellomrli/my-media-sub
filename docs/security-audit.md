# Dependency audit policy

CI runs RustSec on every push. `RUSTSEC-2023-0071` is temporarily ignored because the upstream `rsa` 0.9 line has no patched release. It is transitive through web-push and my-media-sub does not expose an RSA private-key operation to remote callers. The exception must be removed when upstream ships a constant-time replacement.

`RUSTSEC-2026-0185` is fixed by pinning `quinn-proto >= 0.11.15` in `Cargo.lock`.

`RUSTSEC-2026-0285` (rustls TLS 1.3 handshake messages accepted across encryption level boundaries) and `RUSTSEC-2026-0221` (`event-listener` unsound `StackSlot` `Send`/`Sync`) are fixed by raising the locked versions to the patched releases: `rustls >= 0.23.45` (transitive through reqwest/hyper-rustls) and `event-listener >= 5.4.2` (transitive through web-push/isahc/async-channel). Both bumps stay inside the already required minor version, so no manifest constraint changed.
