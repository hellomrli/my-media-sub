# 搜索结果链接有效性检测功能

## 功能说明

为了解决搜索结果中很多链接已失效的问题，现在搜索功能会自动检测所有链接的有效性，只返回可用的资源。

## 实现方式

### 后端改动

1. **搜索 API 增强** (`src/api/search.rs`)
   - 新增 `check_links` 参数
   - 使用 `QuarkShareProbe` 探测每个搜索结果
   - 只返回探测成功（`ok: true`）的链接

2. **主程序初始化** (`src/main.rs`)
   - 从 settings 中读取 `quark_cookie`
   - 创建 `QuarkShareProbe` 实例
   - 传递给搜索路由

3. **路由注册** (`src/api/mod.rs`)
   - 更新 `create_app` 函数签名
   - 传递 `quark_probe` 给搜索路由

### 前端改动

1. **搜索请求** (`static/index.html`)
   - 添加 `check_links: true` 参数
   - 显示 "搜索中，正在检测链接有效性..." 提示
   - 结果为空时提示 "未找到有效资源"

## 工作流程

```
用户输入关键词
    ↓
前端发送搜索请求 (check_links: true)
    ↓
后端调用 PanSou API 获取原始结果
    ↓
对每个结果调用 QuarkShareProbe.probe()
    ↓
过滤掉失效链接 (ok: false)
    ↓
返回有效结果给前端
    ↓
前端显示结果
```

## 探测逻辑

QuarkShareProbe 会检测：
- ✅ 链接格式是否有效
- ✅ 是否能获取分享 token
- ✅ 是否需要提取码
- ✅ 链接是否已失效/被删除

探测结果状态：
- `ok: true` - 链接有效，可以访问
- `ok: false, state: "invalid_url"` - 不是有效的夸克链接
- `ok: false, state: "locked"` - 需要提取码
- `ok: false, state: "bad"` - 链接已失效

## 性能考虑

### 搜索速度
- **原来**：< 1 秒（只调用 PanSou API）
- **现在**：5-15 秒（取决于结果数量）
- 每个链接探测约 200-500ms

### 优化建议

如果觉得太慢，可以使用以下方案：

#### 方案 A：前端控制（推荐）
在前端添加复选框，让用户选择是否启用检测：

```javascript
<label>
  <input type="checkbox" x-model="searchOptions.checkLinks" />
  检测链接有效性（速度较慢）
</label>

// 搜索时
body: JSON.stringify({
  keyword: this.searchQuery,
  limit: 50,
  check_links: this.searchOptions.checkLinks  // 用户可控
})
```

#### 方案 B：减少探测数量
修改 `src/api/search.rs`，只探测前 N 个结果：

```rust
// 只检测前 20 个结果
let results_to_check = results.iter().take(20);
```

#### 方案 C：后台异步检测
搜索立即返回结果，后台异步检测，通过 WebSocket 更新状态（需要较大改动）

## 使用注意事项

1. **需要配置夸克 Cookie**
   - 在"系统设置" -> "夸克网盘"中配置
   - 没有 Cookie 时所有链接都会被过滤掉

2. **搜索变慢是正常的**
   - 需要逐个检测链接
   - 建议搜索时耐心等待

3. **Toast 通知**
   - 搜索开始：显示"正在检测链接有效性"
   - 搜索完成：显示"找到 X 个有效结果"
   - 无结果：提示"未找到有效资源"

## 测试方法

1. 确保已配置夸克 Cookie
2. 搜索一个关键词（如"复仇者联盟"）
3. 等待检测完成（可能需要 10-20 秒）
4. 验证返回的结果都是有效链接

## Git 提交

```
commit 03963f9
feat: 添加搜索结果链接有效性检测功能

- 搜索时自动检测所有链接有效性
- 过滤掉失效的链接
- 使用 QuarkShareProbe 探测链接状态
- 前端显示检测进度提示
```

## 后续优化（可选）

1. 添加并发检测（使用 `tokio::spawn` 并发探测多个链接）
2. 添加超时控制（单个链接探测超时 3 秒）
3. 添加缓存机制（已检测的链接缓存 1 小时）
4. 前端显示检测进度条（X/Y 已检测）

---

**部署状态**: ⏳ Docker 镜像构建中...
**镜像标签**: my-media-sub:v0.6.3-link-check
