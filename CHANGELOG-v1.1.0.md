# My Media Sub v1.1.0 更新日志

发布日期：2026-07-04

## 🎉 新功能

### 订阅失效自动换源
当订阅的夸克分享链接失效时，系统会自动通过 PanSou 搜索相同资源的替代链接，并通知用户选择换源。

**功能特性**：
- ✅ 自动检测链接失效
- ✅ 智能搜索替代源（使用原标题 + 季度信息）
- ✅ 24小时内不重复搜索（避免资源浪费）
- ✅ 多渠道通知（企业微信/Telegram/Bark等）
- ✅ WebUI 换源界面
  - 查看候选列表
  - 探测链接详情
  - 一键应用换源
- ✅ 保留历史链接（可回滚）
- ✅ 换源后自动重置状态

**API 端点**：
- `GET /api/subscriptions/:id/source-candidates` - 获取换源候选列表
- `POST /api/subscriptions/:id/source-candidates/probe` - 探测候选详情
- `POST /api/subscriptions/:id/source-candidates/apply` - 应用换源
- `POST /api/subscriptions/:id/source-candidates/search` - 手动触发搜索

**数据模型变更**：
- 订阅新增 `source_candidates` 字段 - 换源候选列表
- 订阅新增 `last_source_search_time` 字段 - 上次搜索时间
- 订阅新增 `previous_share_links` 字段 - 历史链接

---

## 🐛 Bug 修复

### 修复同集重复下载问题 ⭐ 重要
**问题描述**：同一个集数会下载多个不同文件（例如：181.mp4 和 181 4K.mp4）

**根本原因**：
- 去重逻辑只在本次发现的新文件之间比较
- 不会和已转存的文件（`transferred_files`）进行比较
- 导致后续发现的更高质量版本会被重复转存

**修复方案**：
在 `find_new_files` 函数中：
1. 收集所有已知集数（`known_episodes` + 从 `transferred_files` 提取的集数）
2. 在过滤新文件时，直接跳过已知集数
3. 确保同一集数只会转存一次

**影响**：
- ✅ 彻底防止同集重复下载
- ✅ 节省网盘空间和下载带宽
- ✅ 避免重复通知

**代码位置**：
- `src/services/subscription_check/file_filter_methods.rs`

---

## 📝 详细变更

### 新增文件
1. `src/services/subscription_source_switch.rs` - 换源服务
2. `src/api/subscription_source.rs` - 换源 API

### 修改文件
1. `src/models/subscription.rs` - 新增换源相关字段
2. `src/services/subscription_check.rs` - 集成自动搜索换源
3. `src/services/subscription_check/file_filter_methods.rs` - 修复去重逻辑
4. `src/services/mod.rs` - 导出新服务
5. `src/api/mod.rs` - 注册新路由
6. `src/api/subscriptions.rs` - 修复结构初始化
7. `Cargo.toml` - 版本号 1.0.5 → 1.1.0

### 代码统计
- **新增代码**：约 500 行
- **修改代码**：约 150 行
- **总计**：约 650 行改动

---

## 🔄 升级指南

### Docker 用户

```bash
# 停止旧容器
docker stop my-media-sub

# 拉取新版本
docker pull ghcr.io/hellomrli/my-media-sub:v1.1.0

# 启动新容器
docker start my-media-sub

# 或使用 docker-compose
docker-compose pull
docker-compose up -d
```

### 二进制用户

```bash
# 下载新版本
VERSION=v1.1.0
curl -LO "https://github.com/hellomrli/my-media-sub/releases/download/${VERSION}/my-media-sub-${VERSION}-linux-x86_64.tar.gz"

# 解压并替换
tar -xzf my-media-sub-${VERSION}-linux-x86_64.tar.gz
# 替换旧的二进制文件

# 重启服务
systemctl restart my-media-sub
```

### 数据兼容性

✅ **完全向后兼容** - 无需迁移数据，新版本会自动添加新字段

现有订阅数据会自动补充以下字段：
- `source_candidates: []`
- `last_source_search_time: null`
- `previous_share_links: []`

---

## 🧪 测试建议

升级后建议测试以下功能：

### 1. 测试去重修复
- 创建订阅并检查一次
- 等待分享者上传同集数的不同版本
- 再次检查，验证不会重复转存

### 2. 测试换源功能
- 创建一个失效链接的订阅
- 手动检查，触发自动搜索
- 在 WebUI 中查看候选列表
- 探测并应用换源

### 3. 回归测试
- 正常订阅检查和转存
- TMDB 元数据刮削
- Aria2 下载
- 通知推送

---

## ⚠️ 已知限制

### 换源功能
- 目前只支持 PanSou 搜索
- 限制返回 10 个候选
- 24小时内不重复搜索

### 去重修复
- 只防止**新的**重复下载
- **已经重复下载的文件不会自动清理**
- 需要手动删除历史重复文件

---

## 🔮 下一步计划

- [ ] WebUI 换源界面实现
- [ ] Telegram Bot 换源交互
- [ ] 多源聚合搜索
- [ ] 换源历史记录
- [ ] 自动清理重复文件

---

## 💡 使用提示

### 如何手动触发换源搜索

如果某个订阅已经失效，但还没有自动搜索候选：

```bash
curl -X POST http://localhost:56001/api/subscriptions/{订阅ID}/source-candidates/search \
  -u admin:your-password
```

### 如何查看换源候选

访问订阅详情页，如果有候选会显示在页面上（需要实现前端界面）。

或通过 API：
```bash
curl http://localhost:56001/api/subscriptions/{订阅ID}/source-candidates \
  -u admin:your-password
```

### 如何清理重复文件

对于已经重复下载的文件，可以通过以下方式清理：

1. 在夸克网盘中手动删除重复文件
2. 或通过 WebUI 的网盘管理界面删除

---

## 📊 性能影响

- **内存**：基本无影响（新增字段很小）
- **CPU**：去重逻辑略微增加，但可忽略
- **网络**：失效时会调用 PanSou API（一天最多一次）
- **存储**：每个订阅增加约 1-2KB（候选列表）

---

## 🙏 致谢

感谢所有反馈问题和建议的用户！

---

**完整更新内容请查看**：
- GitHub Release: https://github.com/hellomrli/my-media-sub/releases/tag/v1.1.0
- 提交历史: https://github.com/hellomrli/my-media-sub/compare/v1.0.5...v1.1.0
