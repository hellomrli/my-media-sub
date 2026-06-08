# my-media-sub

微信机器人 / WebUI + PanSou + 夸克网盘 + OpenList + NAS 的影视资源自动化助手。

目标流程：

```text
微信消息或 WebUI 输入：想看 盗梦空间
  ↓
服务调用 PanSou 搜索夸克资源
  ↓
返回候选结果给用户选择
  ↓
用户回复“选 2”或在 WebUI 点击选择
  ↓
转存夸克分享链接到配置的夸克网盘目录
  ↓
OpenList 挂载夸克目录
  ↓
NAS 通过 OpenList 下载/同步到媒体库
```

## 当前能力

- [x] Docker 部署
- [x] HTTP API + Basic Auth
- [x] PanSou 夸克资源搜索 + 多网盘类型
- [x] 微信机器人文本接口雏形
- [x] 暗色 WebUI 控制台（Linear 风格）
- [x] 链接有效性检测 & 夸克文件嗅探（剧集智能识别）
- [x] 订阅规则模型（包含/排除/正则/重命名/only_latest）
- [x] 订阅更新检查 + 转存规划预览
- [x] 通知中心（WebUI 内查看）
- [x] Aria2 手动下载 + 订阅新增自动提交
- [x] 夸克分享链接自动转存到用户网盘
- [x] 后台定时检查订阅更新（FastAPI lifespan）
- [x] OpenList/NAS 自动同步（夸克转存后通过 OpenList 复制到 NAS 挂载目录）
- [ ] 微信机器人平台专用适配器

## 历史版本更新记录

### 2026-06-09

- 新增“我的网盘”页面：支持读取夸克网盘目录、进入子目录、新建文件夹、重命名和删除。
- 优化“我的网盘”页面样式：参考 OpenList 的浅色文件管理器布局，加入路径栏、白色列表卡片和更清晰的文件/文件夹行。
- 简化“我的网盘”文件列表：只显示图标和名称，文件夹整行可点击进入，减少不必要的元信息干扰。
- 优化设置页保存回显：普通字段保存后继续显示，密码/Cookie 等敏感字段显示“已保存”状态但不回显明文。
- 修复夸克网盘子目录读取：目录列表参数对齐 OpenList/夸克当前接口，进入子文件夹可正常加载。
- 修复夸克网盘新建/重命名/删除接口：按 OpenList 当前 Quark Cookie 驱动对齐目录和文件操作参数。
- 新增设置诊断：支持在 WebUI 测试夸克 Cookie、OpenList 登录和 NAS 路径。
- 新增 OpenList/NAS 自动同步：夸克转存成功后可通过 OpenList 复制到 NAS 挂载目录，并返回结构化同步状态。
- 优化订阅流程：搜索结果点击“订阅”后立即进入规则设置，不再需要到订阅列表二次编辑。
- 修复订阅启用触发：在新订阅弹窗保存规则后会立即检查一次并执行转存，不再需要到订阅清单手动点“刷新”。
- 优化订阅转存开关：创建订阅时跟随全局夸克自动转存设置，并可在订阅规则弹窗中单独切换自动转存/仅通知模式。
- 修复订阅转存执行：目标目录和重命名模板会真正应用到夸克转存结果，并等待夸克目录刷新后再执行重命名。
- 对齐夸克客户端兼容性：使用 `https://drive.quark.cn/1/clouddrive`，并安全持久化夸克响应刷新的 Cookie 片段。

## 快速部署

```bash
git clone https://github.com/hellomrli/my-media-sub.git
cd my-media-sub
cp .env.example .env
docker compose up -d --build
```

访问 WebUI：

```text
http://你的服务器:8787/
```

健康检查：

```bash
curl http://127.0.0.1:8787/health
```

## API 示例

### 微信消息入口

```bash
curl -X POST http://127.0.0.1:8787/api/wechat/message \
  -H 'Content-Type: application/json' \
  -d '{"chat_id":"test","text":"想看 盗梦空间"}'
```

### 选择结果

```bash
curl -X POST http://127.0.0.1:8787/api/wechat/message \
  -H 'Content-Type: application/json' \
  -d '{"chat_id":"test","text":"选 1"}'
```

## 安全原则

- 不把 Cookie、Token、OpenList 密码写进仓库
- 使用 `.env` 或部署环境变量
- PanSou 若公网暴露，建议开启认证或加反代访问控制

## 下一步

1. 下载历史持久化 + Aria2 任务状态轮询
2. 外部通知推送（Telegram / 企业微信 Webhook）
3. 微信机器人具体平台适配器

## 认证

设置环境变量后，WebUI 和业务 API 会启用 HTTP Basic 账号密码认证：

```env
APP_USERNAME=admin
APP_PASSWORD=change-me
```

`/health` 保持公开，方便容器健康检查。

## 链接有效性和文件嗅探

搜索时默认会：

1. 调用 PanSou `/api/check/links` 检测夸克分享链接是否有效。
2. 对可疑似有效的夸克分享链接做 best-effort 嗅探，尝试列出分享内文件和目录，并估算连续剧集数。

可通过环境变量关闭：

```env
CHECK_LINKS=false
PROBE_QUARK_FILES=false
```

注意：夸克公开分享接口可能触发风控、验证码、密码或接口变更；这种情况下结果会显示 `locked`、`http_error` 或 `error`，不会中断搜索。

默认会过滤 PanSou 已确认 `bad` 的失效夸克链接。只过滤明确失效的结果；`locked`、`unknown`、`error` 会保留，方便人工判断或稍后重试。

```env
FILTER_BAD_LINKS=true
```


## WebUI Settings

WebUI now includes a settings panel for:

- login username/password
- PanSou base URL
- OpenList base URL
- default cloud disk types
- link check / Quark probe / bad-link filtering toggles
- Aria2 RPC URL, secret, and download directory
- subscription update auto-download toggle
- background subscription scheduler toggle and check interval

Settings are persisted to `/data/settings.json` in Docker.

## Aria2

Configure Aria2 RPC in the WebUI settings or `.env`:

```env
ARIA2_RPC_URL=http://host:6800/jsonrpc
ARIA2_SECRET=
ARIA2_DIR=/downloads
```

After searching, click `Aria2` on a result to send its URL to Aria2. Note: cloud share URLs may not be direct downloadable file URLs; this is mainly useful for direct links, magnets, ed2k, or sources Aria2 can handle.

订阅检查默认仍是“通知优先”。如需在订阅发现新增项目且该订阅 `notify_only=false` 时自动把链接提交给 Aria2，可开启：

```env
AUTO_DOWNLOAD_NEW_SUBSCRIPTION_ITEMS=true
```

自动下载成功后会记录已转存文件，避免后续检查重复提交同一批新增项；失败时会写入通知中心，不会中断订阅检查。

可选开启后台定时检查订阅更新，默认关闭，最小间隔 5 分钟：

```env
SUBSCRIPTION_SCHEDULER_ENABLED=true
SUBSCRIPTION_CHECK_INTERVAL_MINUTES=60
```

定时检查会跳过已停用和已完结订阅；检查失败会写入通知中心，不会终止服务。

## 夸克自动转存

搜索订阅发现新增文件后，可将文件自动转存到用户自己的夸克网盘。需配置夸克 Cookie：

```env
QUARK_COOKIE=你的夸克登录Cookie
QUARK_SAVE_ROOT=/媒体/连续剧
QUARK_SAVE_ENABLED=false
```

Cookie 从浏览器登录 pan.quark.cn 后在开发者工具或应用存储中获取（`__cookie__` 或包含 `__quark__` 的完整 Cookie 字符串）。配置后在 WebUI 设置页开启“夸克自动转存”即可。

项目当前按 OpenList 最新 Quark Cookie 驱动兼容方式调用 `https://drive.quark.cn/1/clouddrive`。夸克响应如果刷新 `__puus` / `__pus`，服务会安全写回本地设置；Cookie 仍属于敏感信息，不会通过设置接口或日志回显。

订阅检查时会自动：
1. 获取分享 token
2. 列出分享内文件
3. 根据订阅规则过滤出需要转存的新文件
4. 在用户夸克网盘创建目标目录（如需）
5. 调用转存接口保存文件
6. 记录已转存文件避免重复
7. 失败写入通知中心，不中断订阅检查

## NAS 自动同步

夸克转存成功后，可选通过 OpenList 将文件自动复制到 NAS 挂载目录。需在 WebUI 设置页或 `.env` 中配置：

```env
OPENLIST_BASE_URL=https://your-openlist.example.com
OPENLIST_USERNAME=你的OpenList登录名
OPENLIST_PASSWORD=你的OpenList密码
NAS_SYNC_ENABLED=true
NAS_SYNC_SOURCE=/QuarkMount          # OpenList 中夸克云盘挂载路径前缀
NAS_SYNC_TARGET=/NASLibrary          # OpenList 中 NAS 挂载路径前缀
```

工作流程：
1. 夸克转存完成后（文件已保存到 `QUARK_SAVE_ROOT/<订阅标题>/`）
2. 登录 OpenList
3. 将 `NAS_SYNC_SOURCE/<订阅标题>/` 中的新增文件复制到 `NAS_SYNC_TARGET/<订阅标题>/`
4. 跳过已存在文件（不会覆盖）
5. 返回结构化同步状态：未启用、未配置、无成功转存、复制成功或复制失败
6. 成功/失败均写入通知中心，失败不中断订阅检查

注意：`NAS_SYNC_SOURCE` 和 `NAS_SYNC_TARGET` 都是**OpenList 内部路径**，对应其挂载的夸克云盘和 NAS 目录。OpenList 的 Quark 驱动本身不支持驱动内 `Copy()`，跨存储复制是否可用取决于 OpenList 服务端和挂载驱动能力；WebUI 设置页提供“测试 OpenList”和“测试 NAS 路径”用于提前检查配置。

常见问题：
- OpenList 登录失败：检查地址、账号、密码，地址不要带 `/api` 后缀。
- NAS 路径不可访问：确认路径是 OpenList 内部路径，例如 `/QuarkMount/媒体/连续剧`，不是本机文件系统路径。
- 夸克 Cookie 失效：重新从浏览器获取完整 Cookie，并使用“测试夸克 Cookie”。
- OpenList 复制失败：确认源挂载和目标挂载均可读写，且当前 OpenList/驱动支持跨存储复制。

## 中文名与订阅

项目中文名：**Lain 的媒体订阅**。

WebUI 已将常见网盘类型本地化展示，例如：夸克网盘、百度网盘、阿里云盘、迅雷网盘、磁力链接等。

订阅 MVP：

- 搜索连续剧后点击结果旁边的“订阅”
- 系统会保存当前夸克分享内已知文件列表
- 在“我的订阅”中点击“检查更新”或“检查全部更新”
- 如果分享目录新增文件，会显示新增文件列表

当前自动下载策略仍是“通知优先”；发现新文件后先提示，后续再接自动转存/Aria2/OpenList 下载策略。

## 参考项目订阅能力记录

参考：

- Cp0204/quark-auto-save：失效分享记录并跳过任务、提取码分享、正则过滤/重命名、任务结束期限、通知推送、媒体库刷新。
- adminpass/aliyundrive-subscribe：订阅检查周期、并发/延迟控制、截止记录 ID、过滤词、保存命名规则、完结状态、Aria2 下载和通知配置。

当前已吸收的能力：

- 订阅状态：`active` / `invalid` / `completed`
- 检查时识别链接疑似失效
- 失效时写入通知中心
- 发现新增文件时写入通知中心
- WebUI 可查看和标记通知已读
- 包含/排除关键词、匹配正则过滤
- 重命名模板和正则替换
- 跳过已转存文件、自动创建目标目录
- 后台定时检查（FastAPI lifespan，可在 WebUI 设置开关和间隔）
- 订阅新增项 Aria2 自动投递
- 夸克分享自动转存到用户网盘
- OpenList/NAS 自动同步和配置诊断

后续计划：外部推送通知（Telegram / 企业微信）、更细的后台任务队列和媒体库刷新。

## 订阅模型增强

订阅已从简单文件名对比升级为规则化模型：

- 媒体类型：连续剧 / 动画（电影不追更）
- 启用 / 停用
- 完结状态
- 季数、当前集、总集数
- 包含关键词、排除关键词、匹配正则
- 只处理最新一集
- 检查历史、最后检查摘要
- 链接失效和新增内容通知

这部分参考了 quark-auto-save、aliyundrive-subscribe 和 ani-rss 的设计，但保留本项目“订阅具体网盘分享目录”的模型。
