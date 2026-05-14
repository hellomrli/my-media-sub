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

## 当前能力

- [x] Docker 部署
- [x] HTTP API
- [x] PanSou 夸克资源搜索
- [x] 微信机器人文本接口雏形
- [x] WebUI 搜索与选择
- [x] 会话内保存最近一次搜索结果
- [ ] 夸克分享链接自动转存
- [ ] OpenList 自动复制/下载到 NAS
- [ ] 微信机器人平台专用适配器

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

1. 接入夸克转存服务
2. 确认 OpenList 中夸克盘挂载路径和 NAS 本地挂载路径
3. 增加 OpenList 复制/同步任务
4. 增加微信机器人具体平台适配器
