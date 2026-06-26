# WebUI 重构计划（my-media-sub）

> 自包含执行手册。任何人/任何工具可照此推进。仓库 `/home/lain/my-media-sub`，工作分支 `webui-redesign`。

## 0. 背景与约束（必读）

- **技术栈**：后端 Rust + Axum；前端纯静态三文件 `static/{index.html, app.js, styles.css}` + Alpine.js + Tailwind。
- **无 npm / 无 bundler**：项目哲学是前端无构建步骤。预编译 CSS 用 **Tailwind standalone CLI**（单个 ~43MB 二进制，不依赖 node_modules）。
- **二进制不入库**：从 https://github.com/tailwindlabs/tailwindcss/releases 下载（v3.4.17，`tailwindcss-linux-x64`），放到 PATH 或用 `TAILWIND_BIN` 指定。
- **在线更新会整目录复制 `static/`**（`src/api/update.rs:386`）：所以 `static/` 必须保持纯静态、产物预编译并提交，部署端不跑构建。
- **后端静态路由**：`ServeDir::new("static")` 递归 serve 子目录（`src/api/mod.rs:99`），`static/vendor/` 可访问。
- **鉴权**：HTTP Basic Auth，默认 `admin` / `change-me`（`src/api/mod.rs`，`basic_auth`）。
- **启动验证**：`docker compose up -d` 或 `cargo run --release`，访问 `http://localhost:56001`。
- **边界**：本重构只改 `static/` + 新增构建配置，**不碰 Rust**。后端测试基线 142+15 不应受影响。

### 已锁定决策（均经用户拍板）

| 维度 | 决定 |
|---|---|
| 范围 | 含技术债清理（高风险档） |
| Tailwind | 预编译静态 CSS，弃用运行时 CDN |
| 工具链 | standalone CLI 单二进制，**不入库**，写进文档 |
| 主题 | 深色 + 浅色双主题 |
| 视觉 | 较大幅度重设计 + 大改布局与交互 |

## 1. 关键技术债（重构动机）

1. **运行时 Tailwind CDN**（生产反模式，无 tree-shake、离线不可用）→ 已在阶段 0 去除。
2. **颜色类名语义错位**：`styles.css` 旧版用 `!important` 把 Tailwind 的 `blue-*` 工具类劫持成 teal（`#11b5a4`）。HTML 里写 `blue`，渲染出来是青绿。共约 **95 处** `blue-*` 类名 + `dark.*` + 灰阶。
3. **单一巨型 `app()` 对象**：`app.js` 3771 行，~120 state + ~20 getter + ~250 方法，无组件化。`formatSize` 定义了两次（`app.js:1018` 和 `app.js:3076`）。
4. **信息密度过高**：网盘列表、后台日志页用 `min-w-[680~760px]` + 横向滚动，移动端体验差；工具栏拥挤。
5. **a11y 缺失**：6 个弹窗无 `role="dialog"`/`aria-modal`/焦点陷阱；纯图标按钮部分缺 `aria-label`；状态仅靠颜色区分。

### app.js 中的动态 class（重要）
`app.js` 里返回 class 字符串的方法**都是完整字面量**（无 `'bg-'+x` 拼接），Tailwind content 扫描能提取，只要 `app.js` 在 `content` 里。涉及方法（阶段 2 语义化时这些也要一起迁移）：
- `app.js:1047-1049, 1354-1357, 1380-1383, 2751-2753, 3004-3008, 3401-3405, 3413, 3587-3589`
- 用到的颜色不止 blue：还有 green / red / amber / yellow / gray。

## 2. 设计系统现状

设计 token 集中在旧 `styles.css` 的 `:root`（已搬进 `tailwind/input.css`）：

```
--app-bg:#08111f  --app-bg-2:#0c1626  --app-surface:#111d2c  --app-surface-2:#162638
--app-surface-3:#1b3146  --app-border:#284159  --app-border-soft:rgba(137,166,194,.18)
--app-muted:#93a4b8  --app-text:#f4f8fb
--app-primary:#11b5a4  --app-primary-hover:#0d9488  --app-secondary:#ff7a59
--app-success:#65d46e  --app-warning:#f5c542  --app-danger:#ff5c7a
--app-cyan:#22d3ee  --app-violet:#b68cff
--app-radius:8px  + 两档阴影
```

复用组件类（`@apply` 重写目标）：`.app-panel` `.app-panel-soft` `.stat-tile` `.btn-primary/secondary/danger` `.chip` `.nav-button` `.section-title` `.field-label` `.segmented-control` `.toolbar-panel` `.control-row` `.list-row` `.dialog-panel` `.empty-state` `.toast` `.settings-tab` `.search-progress`。

## 3. 页面结构索引

`index.html` 1981 行 / `app.js` 3771 行。

- 布局骨架：`index.html:23-101`（shell / 侧栏 / 顶栏 / 移动端导航）
- 7 个主视图：`search`(104) `drive`(239) `downloads`(413) `subscriptions`(529) `transferHistory`(613) `notifications`(737) `settings`(799)
- 设置页 7 个子标签：`app.js:16-24`（基础/夸克/推送/自动化/规则/高级/在线更新）
- 6 个弹窗：`index.html:1433-1976`（升级进度 / 新建文件夹 / 选择转存目录 / 元数据刮削 / aria2 目录 / 订阅配置——最复杂，1630-1924）
- Alpine state：`app.js:3-254`；getter：`256-477`；方法起点 `init` 约 `478`
- 主标签定义：`app.js:6-14`；设置子标签：`app.js:16-24`

## 4. 构建命令

```bash
# 一次性编译（minify）
scripts/build-css.sh
# 或显式指定二进制
TAILWIND_BIN=~/.local/bin/tailwindcss scripts/build-css.sh
# 开发监听模式
scripts/build-css.sh --watch
```

每次改完 `static/index.html` / `static/app.js` / `tailwind/input.css` **都要重新编译** `static/styles.css`，否则新类名不会进产物。

---

## 阶段拆分（逐阶段验证 + 单独提交）

### ✅ 阶段 0：去 CDN + 预编译切换（已完成并提交）

**目标：零视觉差异**，只换资产接入方式。

已落盘：
- 新增 `tailwind.config.js`（`darkMode:'class'`，content 扫 index.html+app.js，保留 `dark.*` 颜色扩展）
- 新增 `tailwind/input.css`（`@tailwind` 三指令 + 原 styles.css 全文原样）
- 新增 `scripts/build-css.sh`（二进制检测 + 下载提示）
- 新增 `static/vendor/alpine.min.js`（vendored 自启动版 Alpine 3.x，44KB）
- `static/styles.css` → 预编译产物（33KB）
- `static/index.html` head 移除两个 CDN script + 内联 config，改本地引用

验证完成：HTTP 200 + MIME 正确、无残留 CDN、产物含全部自定义类 + blue 覆盖层、`node --check` 通过、用户肉眼确认页面正常。

> **注意**：阶段 0 故意保留了 blue 覆盖层和所有旧类名（零视觉差异）。语义化是阶段 2 的事。

---

### 阶段 1：双主题 token 基建

**目标**：把颜色体系改成 CSS 变量驱动、支持深/浅切换；本阶段**不改 HTML**，只改 `tailwind/input.css` + `tailwind.config.js`，视觉仍以深色为默认且尽量保持一致。

任务：
1. 颜色 token 改存 **RGB 分量**（供 Tailwind alpha 用），例：`--c-primary: 17 181 164;`
2. `:root` = 深色一套；`[data-theme="light"]` = 浅色一套。
   - 浅色参考：底 `#f8fafc` / 面 `#ffffff` / 文字深色 / 边框浅灰 / 软阴影；**禁用网格纹理**（`body::before`）、收敛多层渐变。
3. `tailwind.config.js` 的 `theme.extend.colors` 映射语义色：
   ```js
   primary: 'rgb(var(--c-primary) / <alpha-value>)',
   surface: 'rgb(var(--c-surface) / <alpha-value>)',
   // text / muted / border / success / warning / danger / cyan ...
   ```
4. 组件类（`.app-panel` 等）用 `@apply` + `var()` 重写，使其自动跟随主题变量，而非写死深色值。
5. `<html>` 默认仍 `class="dark"`；主题切换将在阶段 3 通过 `data-theme` 实现。

验证：`scripts/build-css.sh` 0 警告；深色视觉与阶段 0 基本一致；手动在 devtools 给 `<html>` 加 `data-theme="light"` 预览浅色不破版。

---

### 阶段 2：HTML + app.js 类名语义化迁移（最大、最易遗漏）

**目标**：把 95 处 `blue-*` + `dark.*` + 裸灰阶替换成语义类，**删除 styles.css 里的 blue 覆盖层**（`!important` 劫持段），让类名与颜色一致。

映射表（建在动手前，系统替换）：
| 旧 | 新（语义） |
|---|---|
| `bg-blue-500` `bg-blue-600/*` | `bg-primary` / `bg-primary/NN` |
| `text-blue-300/400/200` | `text-primary` / `text-primary/NN` |
| `border-blue-500/*` `border-blue-400/*` | `border-primary/NN` |
| `focus:ring-blue-500`（47 处） | `focus:ring-primary` |
| `hover:text-blue-300` `hover:border-blue-500` | `hover:text-primary` / `hover:border-primary` |
| `bg-dark-bg` `bg-dark-card` `bg-dark-hover` | `bg-surface` / `bg-surface-2` 等 |
| `border-dark-border` | `border-border` |
| 裸 `text-gray-*` | `text-text` / `text-muted` |
| `text-purple-300`（实为珊瑚） | `text-secondary` |
| `app.js` 中 green/red/amber/yellow 状态色 | 映射到 `success/danger/warning` 语义色 |

要点：
- **`index.html` 和 `app.js` 都要改**（app.js 里有完整字面量 class，见 §1）。
- 删除 `tailwind/input.css` 里旧的 `.bg-blue-500{...!important}` 等劫持段（阶段 0 原样保留的那块）。
- **最大风险：预编译只含扫描到的类，漏改的类名会静默丢样式不报错。** 缓解：映射表系统替换 + 子代理交叉核对漏改 + 两主题逐页人工核对。
- 改完必须重新编译 CSS。

验证：`scripts/build-css.sh` 0 警告；`node --check static/app.js`；grep 确认无残留 `blue-`/`dark-`/裸 gray；两主题逐页核对。

---

### 阶段 3：主题切换 UI + 持久化

任务：
1. 顶栏（`index.html:76-99`）加 ☀/☾ 切换按钮，带 `aria-label`。
2. `app.js` 加 `theme` state：
   - 初值：读 `localStorage('theme')`，无则跟随 `matchMedia('(prefers-color-scheme: light)')`。
   - 切换写 `document.documentElement.setAttribute('data-theme', ...)`（深色可用默认无属性或 `dark`）+ 存 localStorage。
   - 在 `init()` 里应用初值，注意避免首屏闪烁（可在 `<head>` 内联一小段同步脚本提前设 data-theme）。
3. 确认 `color-scheme` 随主题切换（影响滚动条、表单控件）。

验证：刷新保持选择；跟随系统；切换即时无闪烁；`node --check`。

---

### 阶段 4：大改布局与交互（回归面最大）

**会改动 DOM 结构**，逐页推进，必要时与用户确认单页方向。

候选改造点：
1. **信息密集页减压**：网盘列表（`index.html:335-390`，`min-w-[760px]` 横滚）、后台日志（`grid-cols-12` + `min-w`）——重排为响应式、减少同屏元素。
2. **移动端导航**：当前 7 标签挤 `grid-cols-4`（`index.html:91-98`）会换行——改为可滚动或抽屉式。
3. **订阅弹窗**（`index.html:1630-1924`）：三层嵌套（向导步骤 + 内部 tab + 模式选择）拆解/简化。
4. **统一**：卡片 padding（消除 `p-3/4/5/6` 混用）、圆角、阴影档位。
5. **a11y**：弹窗补 `role="dialog"`/`aria-modal`/焦点陷阱/`Esc` 关闭；图标按钮补 `aria-label`；装饰 SVG 加 `aria-hidden`。
6. 顺手清理 `app.js` 重复的 `formatSize`（保留一处）。

验证：每页两主题人工核对；`node --check`；重新编译 CSS。

---

### 阶段 5：收尾与基线确认

1. 全量 grep 确认无 CDN 残留、无旧类名残留。
2. 重新编译 CSS，确认产物大小合理（参考 ~21KB minified 纯工具类，含自定义层会更大）。
3. `cargo clippy --all-targets -- -D warnings` + `cargo test`，确认后端基线 **142 + 15 全过、0 警告**（应不受影响，因没碰 Rust）。
4. 更新 README：说明前端构建步骤（standalone CLI 下载 + `scripts/build-css.sh`），强调二进制不入库。
5. 两主题端到端人工验收。
6. 合并 `webui-redesign` → `main`（按需开 PR）。

---

## 5. 每阶段通用验证清单

- [ ] `TAILWIND_BIN=~/.local/bin/tailwindcss scripts/build-css.sh` → 0 警告
- [ ] `node --check static/app.js`
- [ ] grep 确认目标旧模式已清除
- [ ] 起服务（`admin`/`change-me`，:56001）两主题逐页肉眼核对
- [ ] 浏览器控制台无红错、Network 无外部 CDN 请求
- [ ] 单独 commit，可独立回滚

## 6. 风险登记

| 风险 | 缓解 |
|---|---|
| 预编译漏类 → 静默丢样式（不报错） | 映射表系统替换 + 子代理核对 + 两主题逐页核对 |
| app.js 字面量 class 未纳入扫描 | 已确认 `app.js` 在 content；均为完整字面量 |
| 阶段 4 改 DOM 引入功能回归 | 逐页推进，单页方向与用户确认 |
| 二进制版本漂移 | 锁定 v3.4.17，README 写明 |
| 在线更新整目录复制 static | 产物预编译并提交，部署端不构建 |
