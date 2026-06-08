# Architecture

## Components

```text
WebUI / WeChat Bot Adapter
  -> FastAPI routes
    -> search_service
      -> InlinePanSouClient-style search aggregator
      -> InlineLinkChecker / QuarkShareProbe
    -> subscription_service
      -> transfer_rule_service
      -> quark_save_service
      -> nas_sync_service
    -> download_service
      -> Aria2 RPC
    -> stores
      -> settings/subscriptions/notifications/downloads JSON
```

## Message Flow

1. User sends: `想看 盗梦空间`.
2. App extracts keyword and runs built-in search sources.
3. Search results are normalized and optionally checked/probed.
4. Bot/WebUI returns numbered candidates.
5. User selects a result or creates a subscription.
6. Subscription rules drive Quark auto-save, rename, and optional NAS copy.
7. Notifications record success/failure without stopping the service.

## Current External Systems

- Quark web APIs for share probing, saving, drive browsing, and file operations.
- Aria2 JSON-RPC for optional download submission.
- Local filesystem/NAS mount paths for sync.

No external PanSou or OpenList service is required.
