# v2.2.5

## 修复

- 编辑保存订阅时不再误清空已有手动排期；仅在明确启用并填写排期时才提交 `manual_schedule`。
- 网盘批量删除补齐后端要求的确认文本，修复“确认后仍删除失败”。
- 搜索结果一键转存会携带分享密码，加密分享可正常转存。
- 网盘列表失败返回真实错误，不再把 Cookie 失效/限流伪装成空目录。
- `find-path` 改为只读解析路径，查找目录时不会在夸克上自动创建缺失路径。
- 设置完成度正确识别自定义分类的 `dir` 字段。
- 搜索进行中会点亮全局 busy 状态（`searching`）。
- 订阅列表加载失败会提示用户，不再静默显示为空。
- 重命名预览在分享探测失败时返回 `probe_warning`，避免误以为已探活。
- 订阅对话框支持浏览网盘选择目标目录；创建订阅会保留所选 `target_fid`。
- 定时调度尊重订阅规则中的 `check_interval_minutes` / `check_weekdays`；手动“检查全部”仍检查全部启用订阅。
- 网盘按 `path` 可列出非根目录；进入/返回目录时强制刷新，减少陈旧缓存。
- 移除 WebUI 中已下线的 STRM 入口提示；OpenAPI 版本与发布版本对齐。

## 兼容性

- JSON Store schema 未变化。
- 可直接从 v2.2.4 升级，保留现有 `data/`。

## 升级

```bash
# Docker
docker compose pull && docker compose up -d

# 二进制：备份 DATA_DIR → 校验新包 → 同时替换二进制和整个 static/ → 保留 data/ → 启动后检查 /health
```

不要只替换二进制而继续使用旧版 `static/`。
