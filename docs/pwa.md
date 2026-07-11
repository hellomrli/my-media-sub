# PWA、离线壳层与缓存安全

my-media-sub 可以安装为桌面或移动端 PWA。PWA 只缓存静态应用壳层，不缓存业务 API、STRM 媒体响应或健康检查。

## 安装

1. 通过 HTTPS 反向代理访问服务，并完成 HTTP Basic Auth；localhost 开发环境也支持 Service Worker。
2. 浏览器触发 `beforeinstallprompt` 后，顶部会出现安装按钮。
3. 安装后可从系统启动器进入，Manifest 提供今日更新、缺集、失败任务、检查全部、下载进度和夸克签到快捷入口。

Safari/iOS 若不提供安装事件，可使用浏览器“添加到主屏幕”。

## 缓存策略

| 请求 | 策略 | 说明 |
|---|---|---|
| HTML 导航 | network-first | 在线响应优先；只有网络请求失败时才回退到认证后缓存的 `/` 壳层。401/403 永不回退。 |
| JS、CSS、图标、字体、Manifest | stale-while-revalidate | 立即使用缓存，并在后台刷新；只缓存成功且未声明 `private/no-store` 的响应。 |
| `/api/*` | network-only | 不读取、不写入 Cache Storage。 |
| `/strm/*` | network-only | 不缓存媒体代理响应和 Token 相关结果。 |
| `/health`、跨域请求、非 GET | network-only | 保持实时或交给浏览器。 |

Cache Storage 使用不含 Authorization header 的规范化静态键。HTTP Basic Auth 401/403 不会写入缓存；离线壳层不包含订阅、Cookie、Token、任务或诊断数据，离线时业务操作仍会失败而不会显示过期 API 数据。

## 更新

- `service-worker.js` 使用显式 `CACHE_VERSION`；浏览器每次启动会检查更新。
- 新 Worker 安装完成后，页面显示“新版本静态资源已就绪”。
- 用户选择立即更新后发送 `SKIP_WAITING`，新 Worker 激活、删除旧版本缓存、接管页面并刷新。
- 服务端对 `/service-worker.js` 返回 `Cache-Control: no-cache` 和 `Service-Worker-Allowed: /`。

## 移动端

390px 断点会缩小页面与 Header 横向留白、压缩移动导航，并将六个快捷入口排为三列。页面保持横向可滚动导航，不隐藏诊断、设置或任务功能。

## 故障排查

- 安装按钮不出现：确认使用 HTTPS/localhost、Manifest 和 Worker 均返回 200，并已通过 Basic Auth。
- 更新没有出现：在开发者工具 Application → Service Workers 中执行 Update，或重新打开页面。
- 离线首次打开失败：至少在线、认证成功地加载一次页面，让 Worker 完成应用壳层预热。
- 修改静态文件后必须提升 `CACHE_VERSION`，否则已安装客户端会继续使用旧缓存。
