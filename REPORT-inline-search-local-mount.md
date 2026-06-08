# Inline Search and Local Mount Integration Report

Date: 2026-06-09
Branch: refactor/inline-search-and-drive

## What Changed

- Created a new Git branch for the larger refactor: `refactor/inline-search-and-drive`.
- Removed runtime dependency on an external PanSou service:
  - Replaced the old `PanSouClient` HTTP wrapper with an in-process search aggregator in `src/clients/pansou.py`.
  - The first built-in sources follow PanSou public-source plugin behavior for quarksoo, quark4k, and pansearch-style sources.
  - Added URL sanity filtering so obvious fake/short Quark paths such as `/s/qyn2` are not returned as usable results.
- Removed runtime dependency on external OpenList APIs:
  - Deleted `src/clients/openlist.py`.
  - Replaced OpenList `/api/fs/copy` NAS sync with local filesystem mount copying in `src/services/nas_sync_service.py`.
  - `NAS_SYNC_SOURCE` and `NAS_SYNC_TARGET` are now local paths, e.g. a mounted Quark directory and a NAS directory.
- Cleaned settings and UI:
  - Removed PanSou/OpenList URL/account/password fields from WebUI settings.
  - Removed OpenList test button and replaced diagnostics with local mount path testing.
  - Legacy `pansou_base_url`, `openlist_base_url`, `openlist_username`, `openlist_password` keys are filtered from public settings even if old local settings files still contain them.
- Improved Aria2 diagnostics:
  - `/api/aria2/test` and `/api/download/aria2` now return a clear HTTP 400 when Aria2 RPC is not configured instead of raw 500 errors.
- Updated docs and examples:
  - `README.md`, `.env.example`, `docs/deployment.md`, `docs/architecture.md`, architecture diagram HTML, and `examples/search_quark.py` now describe the inline search/local mount model.

## Verification Run

Commands and checks executed:

- `python3 -m compileall -q src examples` — PASS
- `node --check static/app.js` — PASS
- `git diff --check` — PASS
- Inline search module direct test for `庆余年` — PASS, returned Quark result from `plugin:quark4k`.
- HTTP `/health` — PASS, reports `search_backend=inline`, `mount_backend=local`.
- WebUI `/` with Basic Auth — PASS.
- `/api/settings` — PASS, no legacy PanSou/OpenList settings are exposed.
- `/api/search` for `庆余年` — PASS, returned usable Quark result.
- Subscription create/update/check/delete flow — PASS.
- Quark drive root list — PASS, returned 6 entries in current test account.
- Quark drive temporary folder create/delete — PASS, temp folder was cleaned up.
- Local mount path diagnostic `/api/settings/test/mount-paths` with `/home/lain -> /tmp` — PASS.
- NAS sync service local-copy unit using temporary directories — PASS, copied test file successfully.
- Aria2 diagnostic — PASS as expected: current local runtime has no Aria2 RPC URL configured, API now returns `400 {"detail":"Aria2 RPC URL 未配置"}` instead of crashing.

## Notes / Limitations

- Aria2 was not configured in `.local-runtime/settings.json`, so I could not submit a real Aria2 download task. I verified the error path and fixed it from 500 to a clean 400 diagnostic.
- Quark cloud share links are not direct downloadable URLs, so Aria2 cannot download Quark disk data directly unless a real direct-link extraction/mount layer exists. Current intended flow is: Quark save -> mounted path appears locally -> optional NAS copy.
- The built-in search aggregator intentionally starts with a smaller set of stable public sources rather than copying PanSou's entire Go plugin ecosystem in one step. This keeps the Python app maintainable and testable.
- Existing `.local-runtime/settings.json` may still contain old legacy keys locally, but they are ignored/filtered and `.local-runtime` is excluded from Git.

## Branch / Commit

- Branch pushed: `refactor/inline-search-and-drive`
- Commit: pending at report generation time; see Git log after push.
