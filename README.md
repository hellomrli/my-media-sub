# my-pansou-openlist-bot

微信机器人 + PanSou + 夸克网盘 + OpenList + NAS 的影视资源自动化助手。

目标流程：

```text
微信消息：想看 盗梦空间
  ↓
机器人调用 PanSou 搜索夸克资源
  ↓
返回候选结果给用户选择
  ↓
用户回复：选 2
  ↓
转存夸克分享链接到个人夸克网盘 /pansou
  ↓
OpenList 挂载夸克目录
  ↓
NAS 通过 OpenList 下载/同步到媒体库
```

## 当前已知环境

- PanSou API: `https://pansou.lxf87.com.cn`
- OpenList: `https://pan.lxf87.com.cn/`
- 夸克保存目录：`/pansou`

## 阶段目标

### MVP 1：搜索与选择

- [ ] 接收微信机器人消息
- [ ] 解析影视搜索意图
- [ ] 调用 PanSou `/api/search`
- [ ] 只返回 `cloud_types=["quark"]` 的结果
- [ ] 保存会话上下文
- [ ] 支持用户回复 `选 1` / `选 2`

### MVP 2：夸克转存

- [ ] 接入夸克 Cookie
- [ ] 解析夸克分享链接
- [ ] 转存到 `/pansou/电影名`
- [ ] 返回转存结果

### MVP 3：OpenList / NAS

- [ ] 确认 OpenList API 登录方式
- [ ] 确认夸克挂载路径
- [ ] 确认 NAS 本地挂载路径
- [ ] 支持从 OpenList 触发 copy/sync 或生成下载路径
- [ ] 完成后通知用户

## 安全原则

- 不把 Cookie、Token、OpenList 密码写进仓库
- 使用 `.env` 或部署环境变量
- PanSou 若公网暴露，建议开启认证或加反代访问控制

## 关键接口

### PanSou 搜索

```http
POST /api/search
Content-Type: application/json
```

```json
{
  "kw": "盗梦空间",
  "res": "merge",
  "cloud_types": ["quark"],
  "src": "all"
}
```

### OpenList 登录

待确认。

### OpenList 文件复制

```http
POST /api/fs/copy
Authorization: <token>
Content-Type: application/json
```

```json
{
  "src_dir": "/quark/pansou/电影/盗梦空间",
  "dst_dir": "/local/Movies",
  "names": ["xxx.mkv"],
  "overwrite": false,
  "skip_existing": true,
  "merge": true
}
```
