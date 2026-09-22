# Dependency audit policy

CI 对每次推送执行两套依赖审计：

- `rustsec/audit-check` 检查安全公告并上报到 GitHub Security 标签页；
- `cargo deny check`（配置见 [`deny.toml`](../deny.toml)）检查 audit-check **不覆盖**
  的三件事：许可证合规、依赖来源白名单、重复依赖与关键 crate 的单版本约束。

## 当前例外：无

自 v2.7.2 起 **没有任何被忽略的公告**，两套审计都在零例外下通过。

### 曾经唯一的一条：`RUSTSEC-2023-0071`（已消除）

`rsa` 0.9 的 Marvin 计时侧信道，上游一直没有修复版本。它曾经是本项目唯一的例外，
依赖路径为：

```
rsa 0.9.10 ← superboring ← jwt-simple ← web-push ← my-media-sub
```

当时的例外理由是「不可达」：VAPID 走 ES256（`p256`），本项目从不执行 RSA 私钥
运算。理由成立，但**例外的唯一来源就是 `web-push`**——而 `web-push` 同时还在一个
以 rustls 为准的项目里编进了第二套 HTTP/TLS 栈：

| 被移除的 crate | 来源 | 说明 |
|---|---|---|
| `rsa` | `superboring` ← `jwt-simple` | 唯一公告例外的来源 |
| `superboring` | `jwt-simple` | **BoringSSL**（C++） |
| `jwt-simple` | `web-push` | VAPID JWT 组装 |
| `isahc` → `curl-sys` → `libnghttp2-sys` / `libz-sys` | `web-push` | **第二套 TLS/HTTP 栈**（C） |
| `web-push` | 直接依赖 | 被 v2.7.2 的自实现取代 |

v2.7.2 用 `src/services/web_push.rs` 取代了 `web-push`：

- `ece`（Mozilla 的 Rust ECE 实现）负责 RFC 8188/8291 的**分组框架**——这部分最
  容易出细微错误，因此复用而不是自己写；通过 `default-features = false` 关掉它默认
  的 OpenSSL 后端；
- 自实现 `ece::crypto::Cryptographer`，密码学后端换成 **`p256` + `ring`**，
  两者本来就已经是本项目的直接依赖，因此**零新增 crate**；
- VAPID（RFC 8292）用 `p256` 签 ES256，JWT 组装与 `aud`/`sub` 校验自行实现。

验证方式见 `src/services/web_push.rs` 的测试模块，共 11 项，其中 6 项直接比对
**RFC 8291 附录 A 的官方中间值**：ECDH 共享密钥、两次 HKDF（IKM / CEK / NONCE）、
AES-128-GCM 密文，以及完整请求体的 86 字节头布局。另有端到端 round-trip
（加密后用自己的私钥解密）、VAPID 签名的自验证（用自己的公钥验证，与推送服务的
校验动作一致）、以及恶意输入与非法密钥的拒绝路径。

> 残留风险：真实推送链路（FCM / Mozilla autopush 等）无法在 CI 中端到端验证，
> 因为它们要求真实的浏览器订阅。升级 `ece` 或调整协议实现后，建议手工对至少一个
> 真实订阅做一次「发送并确认手机收到」的验证。

## 已修复的传递依赖

| 公告 | crate | 状态 |
|---|---|---|
| `RUSTSEC-2026-0285` | `rustls`（TLS 1.3 握手消息跨加密层接受） | 已修：`rustls >= 0.23.45`，当前 `0.23.45`，经 `reqwest` → `hyper-rustls` / `tokio-rustls` |
| `RUSTSEC-2026-0221` | `event-listener`（`StackSlot` 的 `Send`/`Sync` 不健全） | 已修：`>= 5.4.2`，当前 `5.4.2` |
| `RUSTSEC-2026-0185` | `quinn-proto` | 已收敛到 `0.11.18`。**注意**：`quinn`/`quinn-proto` 只作为 `reqwest` 的*可选* HTTP/3 特性出现在 `Cargo.lock`，本项目启用的是 `json, rustls-tls, stream`，因此 `cargo tree -i quinn-proto` 对所有 target 都输出 "nothing to print"——**这些 crate 不在构建图里**。该收敛属于锁文件层面的预防性维护，不是对一个随二进制发布的漏洞的修复，这里如实记录以免高估 |

## 锁文件新鲜度

2026-09-21 执行 `cargo update`，收敛了 132 个 crate 到最新的 semver 兼容版本
（tokio 1.53.1、hyper 1.11.1、rustls-pki-types 1.15.1、webpki-roots 1.0.9、
uuid 1.26.1、regex 1.13.1 等）。`cargo update --dry-run` 现为 0。
Dependabot（`.github/dependabot.yml`）以每周分组的 PR 维持这一状态。