# Architecture

## Components

```text
WeChat Bot Adapter
  -> Bot Orchestrator
    -> PanSou Client
    -> Session Store
    -> Quark Save Service
    -> OpenList Client
    -> Notification Adapter
```

## Message Flow

1. User sends: `想看 盗梦空间`
2. Bot extracts keyword: `盗梦空间`
3. PanSou search returns quark candidates
4. Bot replies numbered list
5. Session store maps chat/user to candidates
6. User sends: `选 2`
7. Bot loads candidate #2
8. Quark Save Service saves share link to `${QUARK_SAVE_ROOT}/<category>/<title>`
9. OpenList exposes saved files
10. NAS copy/sync starts
11. Bot sends completion message

## Open Questions

- 微信机器人具体实现：WeChatPadPro / gewechat / 企业微信 / 其他？
- OpenList admin username/password or token acquisition method?
- OpenList 中夸克盘挂载路径，例如 `/quark`？
- OpenList 中 NAS 本地目录挂载路径，例如 `/local/Movies`？
- 是否已有夸克转存脚本/API？
