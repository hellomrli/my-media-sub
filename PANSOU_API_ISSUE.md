# PanSou API 搜索结果问题说明

## 问题描述

你的 pansou 实例 (https://pansou.lxf87.com.cn) 存在搜索结果质量问题：

### 测试案例：搜索"盗梦空间"

**期望结果：** 返回包含"盗梦空间"或"Inception"的资源

**实际结果：**
- 总共返回 1858 个结果
- 夸克网盘 647 个结果
- ❌ **前 200 个结果中没有一个包含"盗梦空间"关键词**

**返回的无关结果示例：**
1. 同事以上，恋人未满 Office Romance (2026)
2. 挪威足球队：黑马之路 Norges vei tilbake (2026)
3. 每个夏天之后 Every Year After (2026)
4. 邪恶律师 ทนายปีศาจ (2026)
5. 格斗实况(动漫版) 喧嘩独学 (2024)
...

## 问题原因

你的 pansou 实例可能：
1. **TG 频道配置问题** - 配置的 TG 频道质量不高，或者没有影视资源频道
2. **插件未启用** - 没有启用有效的搜索插件（91 个插件中可能只有少数启用）
3. **搜索算法问题** - 全文搜索但排序不合理，相关结果排在很后面

## 解决方案

### 方案 1：修复你的 pansou 实例（推荐）

检查你的 pansou 配置：

```bash
# 1. 检查启用了哪些插件
env | grep ENABLED_PLUGINS

# 2. 检查配置的 TG 频道
env | grep CHANNELS

# 3. 推荐配置
export ENABLED_PLUGINS=wanou,labi,lou1,quark4k,quarksoo,pansearch,alupan,yunpanshare
export CHANNELS=tgsearchers3,quarkshare,panshare123
```

### 方案 2：使用内置搜索源（当前方案）

当前 my-media-sub 已回退到内置搜索源：
- ✅ 结果相关性高
- ❌ 数量较少（通常 1-5 个）

### 方案 3：混合模式（已实现但效果有限）

同时使用内置源 + 远程 API，合并去重：
- 代码已实现（HybridPanSouClient）
- 但由于远程 API 质量问题，实际效果不佳

## 当前状态

my-media-sub 当前使用 `HybridPanSouClient`：
- 优先使用内置源的相关结果
- 补充远程 API 的去重结果
- 搜索"盗梦空间"：返回 1 个相关结果

## 建议

1. **修复 pansou 配置** - 启用更多高质量插件和 TG 频道
2. **测试 pansou 网页** - 在 https://pansou.lxf87.com.cn 网页上测试搜索是否正常
3. **检查 pansou 日志** - 看插件是否正常工作

如果 pansou 实例修复后，只需在 `src/clients/pansou.py` 最后一行改为：
```python
PanSouClient = RemotePanSouClient  # 修复后使用远程 API
```
