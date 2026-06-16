# Next Steps

## Priority 1: Core Stability

- Keep subscription transfer rename behavior under real-world Quark eventual consistency tests.
- Add targeted regression tests when a mocked Quark save/list client is introduced.
- Improve job cancellation semantics for long-running metadata and transfer tasks.

## Priority 2: Async Workflow

- Promote best-effort background push dispatch to a persisted retryable job if push reliability becomes important.
- Add per-channel push retry/backoff and expose failure reasons in notification records.
- Keep subscription checks and transfers free from blocking notification delivery.

## Priority 3: Metadata

- TMDB subscription metadata scraping is now wired as the `metadata_scrape` background job.
- Add Douban metadata support behind the existing provider setting.
- Improve TMDB matching by year, alias, season, and media type.
- Add optional scheduled metadata refresh for subscriptions with missing metadata.

## Priority 4: WebUI Maintainability

- Split `static/index.html` when the no-build single file becomes too hard to maintain.
- Add focused UI states for metadata scraping jobs and push dispatch status.
- Keep controls dense and operational, with subscription, job, and notification flows visible without extra navigation.

## Priority 5: Persistence

- Evaluate SQLite or another embedded store when JSON write contention becomes visible.
- Keep JSON compatibility migrations covered by real-data tests.
