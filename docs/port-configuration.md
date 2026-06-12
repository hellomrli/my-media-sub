# 端口配置更新说明

**更新日期：** 2026-06-12  
**提交：** 7321c2e

---

## 🔧 更新内容

### 端口变更

**修改前：**
- 默认端口：8787
- 绑定地址：0.0.0.0 ✅

**修改后：**
- 默认端口：**50001** ✅
- 绑定地址：0.0.0.0 ✅（保持不变）

---

## 📋 修改的文件

1. ✅ `start_full.sh` - 启动脚本
2. ✅ `README.md` - 项目文档
3. ✅ `docs/browser-history-feature.md` - 功能文档
4. ✅ `docs/browser-history-completed.md` - 完成文档
5. ✅ `test_history.html` - 测试页面

---

## 🌐 局域网访问

### 现在可以从局域网访问

```bash
# 启动服务
cd ~/my-media-sub
./start_full.sh

# 局域网内其他设备访问
http://192.168.50.x:50001/    # x 是服务器的 IP 地址
```

### 查看服务器 IP

```bash
# 查看局域网 IP
ip addr show | grep "inet 192.168"

# 或
ifconfig | grep "inet 192.168"
```

---

## 🧪 测试访问

### 本地访问

```
http://localhost:50001/
http://127.0.0.1:50001/
```

### 局域网访问（从其他设备）

```
http://192.168.50.x:50001/              # 主页
http://192.168.50.x:50001/test_history.html  # 测试页面
http://192.168.50.x:50001/health        # 健康检查
```

### 功能页面直达

```
http://192.168.50.x:50001/?page=searchPage        # 搜索
http://192.168.50.x:50001/?page=downloadsPage     # 下载管理
http://192.168.50.x:50001/?page=subscriptionsPage # 订阅管理
http://192.168.50.x:50001/?page=settingsPage      # 设置
http://192.168.50.x:50001/?page=drivePage         # 网盘管理
http://192.168.50.x:50001/?page=notificationsPage # 通知中心
```

---

## 🚀 启动服务

### 方法 1：使用启动脚本（推荐）

```bash
cd ~/my-media-sub
./start_full.sh
```

### 方法 2：直接命令

```bash
cd ~/my-media-sub
source venv/bin/activate
uvicorn src.app:app --host 0.0.0.0 --port 50001
```

### 方法 3：开发模式（热重载）

```bash
cd ~/my-media-sub
source venv/bin/activate
uvicorn src.app:app --host 0.0.0.0 --port 50001 --reload
```

---

## 📱 移动设备测试

### 手机/平板访问

1. 确保手机/平板与服务器在同一局域网
2. 在手机浏览器输入：`http://192.168.50.x:50001/`
3. 测试响应式布局和触摸操作

### 推荐浏览器

- ✅ iOS Safari
- ✅ Android Chrome
- ✅ 微信内置浏览器
- ✅ 其他现代浏览器

---

## 🔥 防火墙配置（如需要）

### Ubuntu/Debian

```bash
# 允许 50001 端口
sudo ufw allow 50001/tcp
sudo ufw status
```

### CentOS/RHEL

```bash
# 允许 50001 端口
sudo firewall-cmd --permanent --add-port=50001/tcp
sudo firewall-cmd --reload
```

---

## ✅ 验证服务

### 健康检查

```bash
# 本地检查
curl http://localhost:50001/health

# 局域网检查
curl http://192.168.50.x:50001/health
```

### 预期响应

```json
{
  "status": "healthy",
  "version": "0.5.2",
  "timestamp": "2026-06-12T22:02:00+08:00"
}
```

---

## 📊 端口选择理由

### 为什么选择 50001？

1. ✅ **>5000** - 符合要求（避免与系统端口冲突）
2. ✅ **避免常用端口**
   - 5000：Flask 默认端口
   - 50001：相对不常用
3. ✅ **易于记忆** - 简单的数字
4. ✅ **无需 root 权限** - >1024 的端口

---

## 🎉 完成状态

- [x] 修改启动脚本
- [x] 更新所有文档
- [x] 更新测试页面
- [x] Git 提交完成
- [ ] 推送到 GitHub（待手动执行）
- [ ] 测试局域网访问

---

## 📝 下次启动

```bash
cd ~/my-media-sub
./start_full.sh

# 然后访问
http://localhost:50001/              # 本地
http://192.168.50.x:50001/          # 局域网
```

**配置已完成！现在可以从局域网访问和测试了！** 🎊
