# v2.2.4

## 修复

- TMDB 海报、搜索结果和日历缩略图改为通过应用同源接口加载，浏览器不再直接请求 `image.tmdb.org`，避免登录或订阅检查后受第三方图片缓存、DNS、隐私拦截和连接复用影响。
- VPS 图片代理使用共享 HTTP 客户端与有界内存缓存，并向浏览器返回长时间 immutable 缓存头。
- 所有关键 JS/CSS 资源增加与应用版本一致的查询参数，确保被旧 Service Worker 控制的客户端也会获取当前前端代码。
- PWA 缓存代次和预缓存资源同步升级到 v2.2.4。

## 安全性

- 图片代理只允许明确列出的 TMDB 图片尺寸、安全文件名和常见栅格图片扩展名。
- 拒绝路径穿越、SVG/非图片响应和超过 8 MiB 的图片。
- 代理接口继续受 Basic Auth 或只读自动化 Token 保护。

## 兼容性

- JSON Store schema 未变化。
- 可直接从 v2.2.3 升级，保留现有 `data/`。

## 升级

```bash
# Docker
docker compose pull && docker compose up -d

# 二进制：备份 DATA_DIR → 校验新包 → 同时替换二进制和整个 static/ → 保留 data/ → 启动后检查 /health
```

不要只替换二进制而继续使用旧版 `static/`。
