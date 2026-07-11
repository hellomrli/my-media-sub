# HTTPS 反向代理与安全部署

my-media-sub 使用 HTTP Basic Auth，生产环境必须由可信反向代理终止 HTTPS；不要把 56001 端口直接暴露到公网。

## Nginx 示例

```nginx
server {
    listen 443 ssl http2;
    server_name media.example.com;
    ssl_certificate /etc/letsencrypt/live/media.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/media.example.com/privkey.pem;

    client_max_body_size 384m; # Base64 归档上传；解码内容仍受应用内 256 MiB 限制
    location / {
        proxy_pass http://127.0.0.1:56001;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-Proto https;
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_http_version 1.1;
        proxy_buffering off;
    }
}
```

## 安全要求

- 使用至少 12 位、非默认且唯一的 `SERVER_PASSWORD`；推荐 16 位以上并混合字符类型。
- 仅信任由反向代理覆盖的 `X-Forwarded-For`，防火墙应阻止客户端绕过代理直连应用。
- 备份包含 Cookie、Token 等完整业务配置，应加密保存并限制访问。
- `/api/backups/restore` 需要精确确认文本 `RESTORE DATA`；恢复后必须安全重启服务。
- 网盘删除需要与文件 ID/批量数量匹配的确认文本，订阅删除需要确认参数与订阅 ID 一致。
- CSP、`nosniff`、拒绝 iframe、Referrer Policy 和 Permissions Policy 由应用统一返回。
- CI 使用 RustSec 审计依赖；发现高危公告后应先升级依赖再发布。
