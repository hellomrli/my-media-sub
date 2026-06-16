# Architecture

## Current Shape

`my-media-sub` is a Rust/Axum application with a single WebUI entry point and JSON-backed local state. Runtime dependencies are initialized once in `AppContext`, then shared by API routes, the subscription scheduler, and background-capable services.

```text
main.rs
  -> config::Config
  -> app::AppContext
    -> stores
      -> SettingsStore
      -> SubscriptionStore
      -> NotificationStore
      -> JobStore
    -> clients
      -> PanSouClient
      -> QuarkShareProbe / QuarkSaveClient are created per operation with current settings
    -> services
      -> SubscriptionCheckService
      -> SubscriptionTransferService
      -> SubscriptionScheduler
    -> jobs
      -> JobQueue
      -> background worker
    -> api::create_app()
      -> Axum routes
      -> static WebUI
```

## Module Boundaries

- `api/`: HTTP adapters only. Parse request data, call services/stores, return JSON or static UI.
- `app.rs`: composition root. Builds shared stores, clients, services, and background services.
- `clients/`: external system clients. Keep Quark and search protocol details here.
- `jobs/`: async job models, persisted job state, queue submission, and worker execution.
- `services/`: business workflows. Subscription checks, transfers, renaming, push events, and scheduling belong here.
- `store/`: persistence adapters. JSON files and in-memory locks stay behind store APIs.
- `models/`: stable serialized shapes shared by API, store, and services.
- `static/`: WebUI. It should talk to HTTP APIs, not duplicate backend rules.

## Async Direction

The project is already async at the HTTP and service level. Manual transfer now submits a `manual_transfer` job and returns a job id immediately. The next architectural step is to move subscription auto-transfer into the same job pipeline and expose progress updates to the WebUI.

Target direction:

```text
API / Scheduler
  -> enqueue Job
    -> Worker task
      -> Service workflow
      -> Store progress snapshots
      -> Emit notifications / push events
      -> SSE/WebSocket progress stream
```

Recommended job types:

- `manual_transfer`: implemented. Save a Quark share in the background and persist status.
- `subscription.check`: probe shares, detect new files, enqueue transfers.
- `transfer.save`: next. Save subscription files, wait for eventual consistency, rename files.
- `metadata.scrape`: later TMDB/Douban metadata matching.
- `push.dispatch`: optional retryable notification delivery.

## Near-Term Refactor Order

1. Keep `AppContext` as the only place that creates long-lived dependencies.
2. Introduce a `jobs/` module with job models, `JobStore`, and an async worker loop. Done.
3. Move manual transfer into jobs with persisted progress. Done.
4. Move subscription auto-transfer into jobs with persisted progress.
5. Add SSE endpoint for job progress and connect the WebUI transfer progress panel.
6. Split `static/index.html` only when the no-build WebUI becomes hard to maintain.
7. Add metadata scraping as a low-priority async job after core transfer UX is stable.

## Current Constraints

- Local JSON persistence is simple and portable, but it is not a high-concurrency database.
- Quark save APIs can be eventually consistent, so transfer jobs must tolerate delayed file visibility.
- Push channels are best-effort. They should not fail subscription checks or transfers.
- Metadata scraping should never block subscription creation or normal checks.
