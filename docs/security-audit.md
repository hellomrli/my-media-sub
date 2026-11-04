# Dependency audit policy

CI 对每次推送执行两套依赖审计：

- `rustsec/audit-check` 检查安全公告并上报到 GitHub Security 标签页；
- `cargo deny check`（配置见 [`deny.toml`](../deny.toml)）检查 audit-check **不覆盖**
  的三件事：许可证合规、依赖来源白名单、重复依赖与关键 crate 的单版本约束。

## 当前例外：无

自 v2.7.2 起 **没有任何被忽略的公告**，两套审计都在零例外下通过。

### 2026-11-04 / v2.7.7 安全与版本复核

- 核对 [Rust 官方 stable manifest](https://static.rust-lang.org/dist/channel-rust-stable.toml)：本次核验的最新稳定版是 **1.99.1**（manifest 日期 2026-10-15）。开发、CI、Release 与 Docker 构建已统一使用该版本；`Cargo.toml` 的 **1.89 只是 MSRV**，不表示发布产物由旧编译器构建。
- 先按原有版本范围与 Rust 1.89 下限更新 52 个锁定依赖，加入 `utf_types`；再完成 reqwest 0.13 迁移。相对本轮维护前，最终锁文件包数从 248 变为 259；不是 259 个依赖都进入 Linux 发布二进制。
- 重点更新 `uuid 1.29.0`、`hyper 1.12.0` / `hyper-util 0.1.21`、ICU 2.4、`tokio-rustls 0.26.6`、`crypto-bigint 0.7.10` / `elliptic-curve 0.14.3`。这些更新含维护与正确性修复，不将所有更新笼统记作新的漏洞修复。
- `reqwest 0.12.28 → 0.13.5`：按 [0.13 迁移说明](https://github.com/seanmonstar/reqwest/releases/tag/v0.13.0) 显式启用 `query`、选择 `rustls-no-provider`，由统一客户端工厂提供原有 ring / Mozilla 根证书配置。没有改用默认 AWS-LC 或平台证书信任库，也没有放宽证书或主机名校验。`openssl-probe` 是查找证书路径的 Rust crate，不是 `openssl-sys`；构建图仍不含 OpenSSL / libcurl / AWS-LC 后端。
- 客户端隐式重试明确关闭，只有幂等调用继续使用现有最多 3 次的退避策略。初始化失败不再退回丢失超时/重定向限制的默认客户端；新增本地 HTTPS、未知 CA、主机名不匹配、POST 不重放和更新下载白名单测试。`deny.toml` 为 `rustls` 增加单版本约束，避免预配置 TLS 类型与 reqwest 的 rustls 版本分叉。
- [reqwest 0.13.5](https://github.com/seanmonstar/reqwest/releases/tag/v0.13.5) 包含多个匹配代理之间可能选错认证信息的修复；本项目没有自行配置多代理规则，不声称当前部署必然受影响。
- Alpine `3.14.1 → 3.17.5`：来自官方 npm tarball，提取前校验 SHA-512，并保留许可证。对实际包内的 `alpinejs 3.17.5`、`@vue/reactivity 3.5.41`、`@vue/shared 3.5.41` 查询 npm 官方公告接口，返回空公告；临时锁文件上的 `npm audit --registry=https://registry.npmjs.org` 也无已知漏洞。来源与摘要见 [vendor 记录](../static/vendor/README.md)。
- `compression-codecs 0.4.47` 的上游适配引入 Brotli 9 和相应分配器版本；新增 HTTP 压缩协商、Range 与 SSE 回归，验证结果记录在 [本轮路线图](roadmap.md)。Web Push 继续以 RFC 8291 向量、加解密往返和 VAPID 签名验证覆盖密码学依赖变化；设备送达仍需真实订阅手工确认。
- 刷新公告数据库后，`cargo deny --locked check` 的 advisories / bans / licenses / sources 全部通过，`ignore = []` 不变；GitHub Dependabot 查询无未关闭漏洞告警。现有重复版本和未使用许可证许可项仍会告警，不把它们表述为零警告。Release 工作流也执行依赖审计，而不仅依赖 main CI。
- Docker 构建增加 `pull: true` 与 `no-cache-filters: runtime`，运行层执行软件包更新，避免重用过时安全补丁层。保留 bookworm 兼容现有发布二进制；这些措施不等同于完整容器 CVE 扫描，更不会更新宿主机内核。只做 WebUI 在线更新也不会更新镜像系统层，应重新拉取镜像并重建容器。
- 未启用 HTTP/3；迁移 reqwest 后，`quinn` / `quinn-proto` / `quinn-udp` 已从锁文件移除。

### 2026-11-03 复核

- PR #11 更新 `tokio` 至 1.54.1、`serde_json` 至 1.0.154、`uuid` 至 1.28.0、`libc` 至 0.2.191。刷新公告数据库后，`cargo +1.99.1 deny --locked check` 的 advisories / bans / licenses / sources 全部通过，`ignore = []` 保持不变；重复传递依赖仍按既有策略告警。
- 仓库的 GitHub Dependabot 漏洞告警已开启，未开启自动合并。告警扫描与 RustSec CI 互补，不以忽略公告的方式通过门禁。
- Rust 工具链与 Docker builder 同步升级为 1.99.1，最低支持版本仍为 1.89。CI 和 Release 显式读取固定版本，并校验 Dockerfile；PR 新增真实 Docker 构建、启动与认证 API smoke，不再以仅宿主机编译的绿色 CI 代替镜像验证。
- 搜索图片没有新增任意 URL 代理或放宽 `img-src`。外部 HTTP 图片升级到 HTTPS；源站不支持 HTTPS 或图片不可访问时，尝试后续候选，最终退回纯信息卡片。TMDB 图片仍使用原有固定域名、尺寸和文件名受限的同源代理。
- 这些版本更新同时包含常规维护，不能笼统等同于修复一个新的 CVE；已知公告的具体修复下限见下表。本次依赖检查也不等同于宿主机、内核或容器基础系统的完整漏洞扫描。

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
| `RUSTSEC-2026-0387` | `crypto-bigint`（`p256` 的传递依赖） | 已修：`>= 0.7.6`，当前 `0.7.10`；通过将 `p256` 从 0.13.2 升级到 0.14.0（`elliptic-curve 0.14`）获得 |
| `RUSTSEC-2026-0386` | `tokio`（`AsyncFd` I/O safety） | 已修：`>= 1.53.3`，当前 `1.54.1` |
| `RUSTSEC-2026-0382` | `rustls-webpki`（distrusted intermediates） | 已修：`>= 0.103.16`，当前 `0.103.16` |
| `RUSTSEC-2026-0381` | `webpki-roots`（distrusted emSign roots） | 已修：`>= 1.0.10`，当前 `1.0.11` |
| `RUSTSEC-2026-0285` | `rustls`（TLS 1.3 握手消息跨加密层接受） | 已修：`rustls >= 0.23.45`，当前 `0.23.46`，经 `reqwest` → `hyper-rustls` / `tokio-rustls` |
| `RUSTSEC-2026-0221` | `event-listener`（`StackSlot` 的 `Send`/`Sync` 不健全） | 已修：`>= 5.4.2`；当前锁文件已无该 crate |
| `RUSTSEC-2026-0185` | `quinn-proto` | reqwest 0.13 迁移后已从锁文件移除。此前仅为可选 HTTP/3 锁定项，不在实际构建图内；本项目始终未启用 HTTP/3，不把锁文件维护表述成发布二进制中可达漏洞的修复 |

## 锁文件新鲜度

2026-10-24 定点更新了四条新公告涉及的依赖：`tokio 1.53.3`、`rustls-webpki 0.103.16`、
`webpki-roots 1.0.11`，并将 `p256` 从 0.13.2 升级到 0.14.0
（`elliptic-curve 0.14` → `crypto-bigint 0.7.6`）。`cargo deny check` 的公告、
许可证、来源与重复依赖规则全部通过，例外仍为 0。

当时的 `cargo update --dry-run` 显示约 50 个兼容更新，未与该轮安全修复混合。
2026-11-03 先吸纳四项更新，2026-11-04 再完成剩余依赖维护，见本页复核记录。
2026-11-04 联网 dry-run 复核为 0 个约束内待更新依赖，reqwest 也已升至 0.13.5。
唯一落后于同系列最新版本的项是 `matchit 0.8.4`（上游有 0.8.6），因为当前
`axum 0.8.9` 精确要求 `=0.8.4`；RustSec 未命中该版本公告，不通过 fork / patch
绕过框架的路由兼容约束。后续跟随 axum 正式更新再收敛。

后续检查和更新应保留项目的 MSRV 下限，而不是只按开发工具链选择版本；若安全
修复确实需要提高 MSRV，应明确调整声明并重新验证，而不能为了兼容而保留漏洞：

```bash
cargo update --dry-run --config 'resolver.incompatible-rust-versions="fallback"'
cargo update --config 'resolver.incompatible-rust-versions="fallback"'
cargo deny --locked check
CARGO_TARGET_DIR=target/msrv cargo +1.89.0 check --all-targets --all-features --locked
```

最后一条需要先安装 Rust 1.89.0。解析器只依据依赖声明的 `rust-version` 选择候选，
不能代替真实 MSRV 编译与完整测试；本地验证步骤见 [README](../README.md)。
Dependabot（`.github/dependabot.yml`）继续以每周分组 PR 维护锁文件，不自动合并。
