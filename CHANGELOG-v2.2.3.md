# v2.2.3

## 修复

- 修复批量检查完成并重新加载订阅数据时，Alpine 按相同订阅 ID 复用图片 DOM 节点，使 `remote-image-failed` 继续生效并将实际已存在的海报显示为透明的问题。
- 订阅数据刷新后主动恢复失败图片节点，清理失败、重试计数和旧版本遗留的 `hidden` 状态，然后使用新的 cache-busting URL 重试。
- 新增相同 URL 和相同 DOM 节点被复用时的回归测试，同时确保正常图片不被重复请求。
- 提升 PWA 缓存版本，确保客户端获取本次图片节点恢复逻辑。

## 兼容性

- JSON Store schema 未变化。
- 可直接从 v2.2.2 升级，保留现有 `data/`。

## 升级

```bash
# Docker
docker compose pull && docker compose up -d

# 二进制：备份 DATA_DIR → 校验新包 → 同时替换二进制和整个 static/ → 保留 data/ → 启动后检查 /health
```

不要只替换二进制而继续使用旧版 `static/`。详细步骤与回滚见对应版本的升级指南。
