//! Web Push 的最小自实现：VAPID (ES256) 签名、RFC 8291 `aes128gcm` 载荷加密、
//! 以及通过现有 `reqwest` 连接池投递。
//!
//! # 为什么不用 `web-push`
//!
//! 原实现依赖 `web-push`，它带来了三样本项目并不想要的东西：
//!
//! 1. **唯一的 RustSec 例外**：`rsa` 0.9 的 Marvin 计时侧信道
//!    （`RUSTSEC-2023-0071`，上游无修复版本）经
//!    `rsa ← superboring ← jwt-simple ← web-push` 引入。本项目从不做 RSA 私钥
//!    运算（VAPID 走 ES256），因此该公告不可达——但它迫使审计长期保留一条例外，
//!    而这条例外的**唯一来源**就是 `web-push`。
//! 2. **第二套 TLS/HTTP 栈**：`web-push` 用 `isahc`，后者链接 `libcurl` /
//!    `libnghttp2` / `libz`，在 Cargo 里编进第二套 C 实现；本项目其余部分一律
//!    走 rustls。
//! 3. **BoringSSL**：`superboring` 是 C++ 实现。
//!
//! 换成自实现后，上面这些依赖（`web-push`、`isahc`、`curl-sys`、`libnghttp2-sys`、
//! `libz-sys`、`superboring`、`jwt-simple`、`rsa`）全部消失，`security-audit.md`
//! 里的例外也随之删除，项目回到单一 rustls 栈。
//!
//! # 实现策略：复用 `ece` 的框架，只替换密码学后端
//!
//! `ece`（Mozilla 的 Rust ECE 实现）负责 RFC 8188/8291 的**分组框架**——这部分
//! 最容易出细微错误。它的默认后端是 OpenSSL（又要引入一套 C 依赖），因此这里
//! 通过实现 [`Cryptographer`] 换成纯 Rust 的 `p256` + `ring`，
//! 两个都已经是本项目的直接依赖，**零新增 crate**。
//!
//! 正确性由 RFC 8291 附录 A 的官方中间值逐项验证（见本模块测试）：
//! ECDH 共享密钥、两次 HKDF、CEK、NONCE、AES-GCM 密文全部对齐。

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ece::crypto::{Cryptographer, EcKeyComponents, LocalKeyPair, RemotePublicKey};
use ece::Error as EceError;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use std::any::Any;
use std::sync::Once;
use std::time::{SystemTime, UNIX_EPOCH};

/// VAPID JWT 的有效期。规范上限 24 小时，取 12 小时留出时钟偏移余量。
const VAPID_TOKEN_TTL_SECONDS: u64 = 12 * 60 * 60;
/// 单条推送的 TTL（秒）。与旧实现保持一致。
pub const PUSH_TTL_SECONDS: u32 = 3600;

/// `ece::Error` 的后端错误变体（`CryptoError`）**不带消息**，直接返回会把诊断信息
/// 丢掉。因此这里先把原因写进日志，再返回该变体；同时 `encrypt_payload` 会在调用
/// `ece` 之前完成全部可预检的校验，让这条路径基本不可达。
fn ece_error(message: impl std::fmt::Display) -> EceError {
    tracing::warn!("Web Push 密码学后端错误: {message}");
    EceError::CryptoError
}

// ─────────────────────────── 密码学后端 ───────────────────────────

/// 用 `p256` + `ring` 实现的 ECE 密码学后端（纯 Rust，无 C 依赖）。
struct RustCryptographer;

/// P-256 私钥 + 未压缩公钥的本地密钥对。
struct P256KeyPair {
    secret: p256::SecretKey,
    /// 未压缩点（65 字节，0x04 开头）。预先算好，因为 `LocalKeyPair` 的接口是
    /// `&self`，而 `public_key()` 每次都会做一次标量乘。
    public: Vec<u8>,
}

struct P256RemotePublic {
    /// 未压缩点；`ring`/`p256` 都会在导入时校验它确实在 P-256 上
    /// （RFC 8291 §7 明确要求这一步，否则攻击者可借无效公钥提取私钥）。
    uncompressed: Vec<u8>,
    point: p256::PublicKey,
}

impl RemotePublicKey for P256RemotePublic {
    fn as_raw(&self) -> ece::Result<Vec<u8>> {
        Ok(self.uncompressed.clone())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl LocalKeyPair for P256KeyPair {
    fn pub_as_raw(&self) -> ece::Result<Vec<u8>> {
        Ok(self.public.clone())
    }
    fn raw_components(&self) -> ece::Result<EcKeyComponents> {
        // 本项目只加密、不解密，因此没有调用方需要这条路径；`ece::decrypt`
        // 会用到它，但我们不使用该函数。
        Err(ece_error("本地只实现 Web Push 加密端，不支持导出密钥分量"))
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Cryptographer for RustCryptographer {
    fn generate_ephemeral_keypair(&self) -> ece::Result<Box<dyn LocalKeyPair>> {
        let secret = p256::SecretKey::random(&mut p256::elliptic_curve::rand_core::OsRng);
        let public = secret
            .public_key()
            .to_encoded_point(false)
            .as_bytes()
            .to_vec();
        Ok(Box::new(P256KeyPair { secret, public }))
    }

    fn import_key_pair(&self, _components: &EcKeyComponents) -> ece::Result<Box<dyn LocalKeyPair>> {
        Err(ece_error("本地只实现 Web Push 加密端，不支持导入密钥对"))
    }

    fn import_public_key(&self, raw: &[u8]) -> ece::Result<Box<dyn RemotePublicKey>> {
        // `from_sec1_bytes` 会校验点在曲线上且非无穷远点。
        let point = p256::PublicKey::from_sec1_bytes(raw)
            .map_err(|error| ece_error(format!("公钥不是合法的 P-256 点: {error}")))?;
        Ok(Box::new(P256RemotePublic {
            uncompressed: raw.to_vec(),
            point,
        }))
    }

    fn compute_ecdh_secret(
        &self,
        remote: &dyn RemotePublicKey,
        local: &dyn LocalKeyPair,
    ) -> ece::Result<Vec<u8>> {
        let remote = remote
            .as_any()
            .downcast_ref::<P256RemotePublic>()
            .ok_or_else(|| ece_error("公钥类型不匹配"))?;
        let local = local
            .as_any()
            .downcast_ref::<P256KeyPair>()
            .ok_or_else(|| ece_error("密钥对类型不匹配"))?;
        let shared =
            p256::ecdh::diffie_hellman(local.secret.to_nonzero_scalar(), remote.point.as_affine());
        Ok(shared.raw_secret_bytes().to_vec())
    }

    fn hkdf_sha256(
        &self,
        salt: &[u8],
        secret: &[u8],
        info: &[u8],
        len: usize,
    ) -> ece::Result<Vec<u8>> {
        hkdf_sha256(salt, secret, &[info], len).map_err(ece_error)
    }

    fn aes_gcm_128_encrypt(&self, key: &[u8], iv: &[u8], data: &[u8]) -> ece::Result<Vec<u8>> {
        // 返回 `密文 || 认证标签`，与 ECE 框架的期望一致。
        aes_gcm_128_encrypt(key, iv, data).map_err(ece_error)
    }

    fn aes_gcm_128_decrypt(
        &self,
        _key: &[u8],
        _iv: &[u8],
        _ciphertext_and_tag: &[u8],
    ) -> ece::Result<Vec<u8>> {
        Err(ece_error("本地只实现 Web Push 加密端，不支持解密"))
    }

    fn random_bytes(&self, dest: &mut [u8]) -> ece::Result<()> {
        use ring::rand::SecureRandom;
        ring::rand::SystemRandom::new()
            .fill(dest)
            .map_err(|_| ece_error("系统随机数生成失败"))
    }
}

/// 只注册一次；`ece` 的密码学后端是进程级全局单例。
fn install_cryptographer() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        // 失败意味着别人先注册过——只要注册的是等价实现就无所谓，
        // 因此这里不当作错误，只记录。
        if let Err(error) = ece::crypto::set_cryptographer(&RustCryptographer) {
            tracing::debug!("ECE 密码学后端已被注册: {error}");
        }
    });
}

// ─────────────────────────── 密码学原语 ───────────────────────────

/// ring 的 `KeyType` 只对 `Algorithm` 实现，因此为自己的输出长度造一个小类型。
struct HkdfLen(usize);

impl ring::hkdf::KeyType for HkdfLen {
    fn len(&self) -> usize {
        self.0
    }
}

/// HKDF-SHA256（RFC 5869）。`salt` 为空时 ring 会退化为全零盐，符合 RFC。
fn hkdf_sha256(salt: &[u8], ikm: &[u8], info: &[&[u8]], len: usize) -> Result<Vec<u8>, String> {
    let prk = ring::hkdf::Salt::new(ring::hkdf::HKDF_SHA256, salt).extract(ikm);
    let okm = prk
        .expand(info, HkdfLen(len))
        .map_err(|_| "HKDF 扩展失败".to_string())?;
    let mut out = vec![0u8; len];
    okm.fill(&mut out)
        .map_err(|_| "HKDF 输出长度不匹配".to_string())?;
    Ok(out)
}

/// AES-128-GCM 加密，返回 `密文 || 16 字节认证标签`。
fn aes_gcm_128_encrypt(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
    let unbound = ring::aead::UnboundKey::new(&ring::aead::AES_128_GCM, key)
        .map_err(|_| format!("AES-128-GCM 密钥长度非法（期望 16，实际 {}）", key.len()))?;
    let sealing = ring::aead::LessSafeKey::new(unbound);
    let nonce = ring::aead::Nonce::try_assume_unique_for_key(iv)
        .map_err(|_| format!("GCM nonce 长度非法（期望 12，实际 {}）", iv.len()))?;
    let mut in_out = data.to_vec();
    sealing
        .seal_in_place_append_tag(nonce, ring::aead::Aad::empty(), &mut in_out)
        .map_err(|_| "AES-128-GCM 加密失败".to_string())?;
    Ok(in_out)
}

// ─────────────────────────── 对外接口 ───────────────────────────

/// 加密一条 Web Push 载荷（RFC 8291 `aes128gcm`）。
///
/// `p256dh` 与 `auth` 是浏览器订阅时给出的 base64url 值，直接来自
/// `PushSubscription.getKey()`。
pub fn encrypt_payload(p256dh: &str, auth: &str, plaintext: &[u8]) -> Result<Vec<u8>, String> {
    install_cryptographer();
    let public = URL_SAFE_NO_PAD
        .decode(p256dh)
        .map_err(|error| format!("订阅公钥不是合法 base64url: {error}"))?;
    let auth = URL_SAFE_NO_PAD
        .decode(auth)
        .map_err(|error| format!("订阅 auth secret 不是合法 base64url: {error}"))?;
    if auth.len() != 16 {
        return Err(format!("auth secret 长度应为 16 字节，实际 {}", auth.len()));
    }
    // 在进入 ece 之前自行校验公钥确实在 P-256 上：RFC 8291 §7 要求接收方与
    // 发送方都必须做这一步（未校验的公钥可被用来提取私钥）。这样也能让 ece
    // 内部的 `CryptoError` 变成真正不可达，错误信息保持可读。
    p256::PublicKey::from_sec1_bytes(&public)
        .map_err(|error| format!("订阅公钥不是合法的 P-256 点: {error}"))?;
    if plaintext.is_empty() {
        return Err("推送载荷不能为空（RFC 8291 不接受零长度明文）".to_string());
    }
    ece::encrypt(&public, &auth, plaintext)
        .map_err(|error| format!("Web Push 载荷加密失败: {error}（详细原因见上方日志）"))
}

/// 生成 VAPID `Authorization` 头的值（RFC 8292）。
///
/// 格式为 `vapid t=<jwt>, k=<公钥>`；JWT 用 ES256 签名，`aud` 取推送端点的
/// origin，`sub` 是 `mailto:` 或站点 URL。
pub fn vapid_authorization(
    private_key_b64: &str,
    public_key_b64: &str,
    endpoint: &str,
    subject: &str,
) -> Result<String, String> {
    let audience = endpoint_origin(endpoint)?;
    let subject = normalize_subject(subject);

    let private_bytes = URL_SAFE_NO_PAD
        .decode(private_key_b64)
        .map_err(|error| format!("VAPID 私钥不是合法 base64url: {error}"))?;
    let signing_key = p256::ecdsa::SigningKey::from_slice(&private_bytes)
        .map_err(|error| format!("VAPID 私钥不是合法的 P-256 标量: {error}"))?;

    let header = URL_SAFE_NO_PAD.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "系统时间早于 UNIX 纪元".to_string())?
        .as_secs()
        + VAPID_TOKEN_TTL_SECONDS;
    // `serde_json` 保证 aud/sub 中的引号与反斜杠被正确转义。
    let claims = serde_json::json!({
        "aud": audience,
        "exp": exp,
        "sub": subject,
    })
    .to_string();
    let claims = URL_SAFE_NO_PAD.encode(claims.as_bytes());

    let signing_input = format!("{header}.{claims}");
    use p256::ecdsa::signature::Signer;
    // ES256 在 JWS 里用固定的 64 字节 r||s（不是 DER），`Signature::to_bytes`
    // 正好给出这个形式。
    let signature: p256::ecdsa::Signature = signing_key.sign(signing_input.as_bytes());
    let signature = URL_SAFE_NO_PAD.encode(signature.to_bytes());

    let public_key = URL_SAFE_NO_PAD
        .decode(public_key_b64)
        .map_err(|error| format!("VAPID 公钥不是合法 base64url: {error}"))?;
    if public_key.len() != 65 || public_key[0] != 0x04 {
        return Err("VAPID 公钥应为 65 字节未压缩点（0x04 开头）".to_string());
    }

    Ok(format!(
        "vapid t={signing_input}.{signature}, k={public_key_b64}"
    ))
}

/// 从推送端点取出 origin，作为 JWT 的 `aud`。
///
/// 推送服务会校验 `aud` 与端点同源，因此这里必须严格取 scheme+host+port。
fn endpoint_origin(endpoint: &str) -> Result<String, String> {
    let url =
        reqwest::Url::parse(endpoint).map_err(|error| format!("推送端点不是合法 URL: {error}"))?;
    let host = url
        .host_str()
        .ok_or_else(|| "推送端点缺少主机名".to_string())?;
    let scheme = url.scheme();
    if scheme != "https" {
        return Err(format!("推送端点必须使用 https，实际为 {scheme}"));
    }
    Ok(match url.port() {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    })
}

/// VAPID `sub` 必须是 `mailto:` 或 URL；为空时退化成一个合法的占位值，
/// 否则推送服务会以 403 拒绝整条消息。
fn normalize_subject(subject: &str) -> String {
    let trimmed = subject.trim();
    if trimmed.starts_with("mailto:") || trimmed.starts_with("https://") {
        return trimmed.to_string();
    }
    if trimmed.contains('@') {
        return format!("mailto:{trimmed}");
    }
    // 兜底：用 example.invalid（RFC 2606 保留域，不会指向真实主机）。
    "mailto:admin@example.invalid".to_string()
}

/// 载荷加密后的字节数上限。
///
/// RFC 8291 §4：推送服务不要求支持超过 4096 字节的载荷；扣掉 86 字节头、
/// 1 字节填充分隔符与 16 字节认证标签，明文上限是 3993 字节。这里对**密文**
/// 做上限检查，超限直接拒绝而不是让推送服务返回 413。
pub const MAX_ENCRYPTED_BYTES: usize = 4096;

#[cfg(test)]
mod tests {
    use super::*;

    // ── RFC 8291 附录 A 的官方中间值 ──────────────────────────────────────
    //
    // 这些常量逐字来自 RFC 文本，用来验证我们自己的密码学后端。
    // 注意 `ece::encrypt` 内部自行生成临时密钥与盐，因此无法直接复现 RFC 的
    // 最终密文；改为逐项验证原语，覆盖面等价（ECDH → 两次 HKDF → CEK/NONCE
    // → AES-GCM 全部对齐）。
    /// `aes128gcm` 记录大小。RFC 8291 §4 要求大于「明文 + 填充分隔符 + 认证标签」。
    /// `ece` 内部固定用 4096，这里只在复现 RFC 的 86 字节头布局时用到。
    const RECORD_SIZE: u32 = 4096;
    const RFC_PLAINTEXT: &[u8] = b"When I grow up, I want to be a watermelon";
    const RFC_AS_PRIVATE: &str = "yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw";
    const RFC_AS_PUBLIC: &str =
        "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8";
    const RFC_UA_PUBLIC: &str =
        "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
    const RFC_SALT: &str = "DGv6ra1nlYgDCS1FRnbzlw";
    const RFC_AUTH_SECRET: &str = "BTBZMqHH6r4Tts7J_aSIgg";
    /// 期望的 ECDH 共享密钥。
    const RFC_ECDH_SECRET: &str = "kyrL1jIIOHEzg3sM2ZWRHDRB62YACZhhSlknJ672kSs";
    /// 期望的 IKM（key_info 那一轮 HKDF-Expand 的输出）。
    const RFC_IKM: &str = "S4lYMb_L0FxCeq0WhDx813KgSYqU26kOyzWUdsXYyrg";
    const RFC_CEK: &str = "oIhVW04MRdy2XN9CiKLxTg";
    const RFC_NONCE: &str = "4h_95klXJ5E_qnoN";
    /// 期望的密文（含 16 字节认证标签）。
    const RFC_CIPHERTEXT: &str =
        "8pfeW0KbunFT06SuDKoJH9Ql87S1QUrdirN6GcG7sFz1y1sqLgVi1VhjVkHsUoEsbI_0LpXMuGvnzQ";
    /// 期望的完整请求体（86 字节头 + 密文）。
    const RFC_BODY: &str = "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN";

    fn b64(value: &str) -> Vec<u8> {
        URL_SAFE_NO_PAD.decode(value).unwrap()
    }

    /// ECDH 共享密钥必须与 RFC 一致。用发送方私钥 × 接收方公钥。
    #[test]
    fn rfc8291_ecdh_secret_matches() {
        install_cryptographer();
        let crypto = RustCryptographer;
        let local_secret = p256::SecretKey::from_slice(&b64(RFC_AS_PRIVATE)).unwrap();
        let local = P256KeyPair {
            public: local_secret
                .public_key()
                .to_encoded_point(false)
                .as_bytes()
                .to_vec(),
            secret: local_secret,
        };
        let remote = crypto.import_public_key(&b64(RFC_UA_PUBLIC)).unwrap();
        let secret = crypto.compute_ecdh_secret(remote.as_ref(), &local).unwrap();
        assert_eq!(
            URL_SAFE_NO_PAD.encode(&secret),
            RFC_ECDH_SECRET,
            "ECDH 共享密钥与 RFC 8291 附录 A 不符"
        );
    }

    /// 反方向（接收方私钥 × 发送方公钥）必须得到同一个共享密钥——
    /// 这是整条链路成立的前提。
    #[test]
    fn rfc8291_ecdh_is_symmetric() {
        let crypto = RustCryptographer;
        let ua_secret =
            p256::SecretKey::from_slice(&b64("q1dXpw3UpT5VOmu_cf_v6ih07Aems3njxI-JWgLcM94"))
                .unwrap();
        let ua = P256KeyPair {
            public: ua_secret
                .public_key()
                .to_encoded_point(false)
                .as_bytes()
                .to_vec(),
            secret: ua_secret,
        };
        let as_public = crypto.import_public_key(&b64(RFC_AS_PUBLIC)).unwrap();
        let from_ua = crypto.compute_ecdh_secret(as_public.as_ref(), &ua).unwrap();
        assert_eq!(URL_SAFE_NO_PAD.encode(&from_ua), RFC_ECDH_SECRET);
    }

    /// 两轮 HKDF 的中间值（IKM、CEK、NONCE）必须与 RFC 一致。
    #[test]
    fn rfc8291_key_derivation_matches() {
        let auth_secret = b64(RFC_AUTH_SECRET);
        let ecdh_secret = b64(RFC_ECDH_SECRET);

        // key_info = "WebPush: info" || 0x00 || ua_public || as_public
        let mut key_info = b"WebPush: info\0".to_vec();
        key_info.extend_from_slice(&b64(RFC_UA_PUBLIC));
        key_info.extend_from_slice(&b64(RFC_AS_PUBLIC));

        // HKDF-Extract(salt=auth_secret, IKM=ecdh_secret) 后 Expand 出 32 字节 IKM。
        let ikm = hkdf_sha256(&auth_secret, &ecdh_secret, &[&key_info], 32).unwrap();
        assert_eq!(
            URL_SAFE_NO_PAD.encode(&ikm),
            RFC_IKM,
            "第二轮 IKM 与 RFC 8291 附录 A 不符"
        );

        // RFC 8188 的 CEK / NONCE 派生。
        let salt = b64(RFC_SALT);
        let cek = hkdf_sha256(&salt, &ikm, &[b"Content-Encoding: aes128gcm\0"], 16).unwrap();
        assert_eq!(URL_SAFE_NO_PAD.encode(&cek), RFC_CEK, "CEK 与 RFC 不符");

        let nonce = hkdf_sha256(&salt, &ikm, &[b"Content-Encoding: nonce\0"], 12).unwrap();
        assert_eq!(
            URL_SAFE_NO_PAD.encode(&nonce),
            RFC_NONCE,
            "NONCE 与 RFC 不符"
        );
    }

    /// AES-128-GCM 密文（含标签）必须与 RFC 一致，且填充分隔符为 0x02。
    #[test]
    fn rfc8291_aes_gcm_ciphertext_matches() {
        let cek = b64(RFC_CEK);
        let nonce = b64(RFC_NONCE);
        let mut plaintext = RFC_PLAINTEXT.to_vec();
        plaintext.push(0x02); // RFC 8291 的填充分隔符

        let ciphertext = aes_gcm_128_encrypt(&cek, &nonce, &plaintext).unwrap();
        assert_eq!(
            URL_SAFE_NO_PAD.encode(&ciphertext),
            RFC_CIPHERTEXT,
            "AES-128-GCM 密文与 RFC 8291 附录 A 不符"
        );
        assert_eq!(
            ciphertext.len(),
            plaintext.len() + 16,
            "密文长度应为明文 + 16 字节认证标签"
        );
    }

    /// 完整请求体的结构：86 字节头 + 密文，且头里的 salt/keyid 与输入一致。
    #[test]
    fn rfc8291_body_layout_is_reproducible() {
        let salt = b64(RFC_SALT);
        let as_public = b64(RFC_AS_PUBLIC);
        // aes128gcm 头：salt(16) || rs(4, 大端) || idlen(1) || keyid
        let mut header = salt.clone();
        header.extend_from_slice(&RECORD_SIZE.to_be_bytes());
        header.push(as_public.len() as u8);
        header.extend_from_slice(&as_public);
        assert_eq!(header.len(), 86, "aes128gcm 头应为 86 字节");

        let ciphertext = b64(RFC_CIPHERTEXT);
        let mut body = header;
        body.extend_from_slice(&ciphertext);
        assert_eq!(
            URL_SAFE_NO_PAD.encode(&body),
            RFC_BODY,
            "完整请求体与 RFC 8291 §5 不符"
        );
    }

    /// 端到端round-trip：用 `ece` 走完整流程加密，再用接收方私钥解密。
    ///
    /// 这一步验证的是**分组框架**（头、padding、记录），上面的测试验证的是
    /// **密码学原语**；两者合起来覆盖整条链路。
    #[test]
    fn payload_round_trips_through_ece_framing() {
        use ece::crypto::Cryptographer as _;
        install_cryptographer();

        let crypto = RustCryptographer;
        // 模拟浏览器：生成接收方密钥对与 auth secret
        let receiver = crypto.generate_ephemeral_keypair().unwrap();
        let mut auth = [0u8; 16];
        crypto.random_bytes(&mut auth).unwrap();
        let receiver_public = receiver.pub_as_raw().unwrap();

        let plaintext = b"{\"title\":\"test\",\"body\":\"hello\"}";
        let encrypted = encrypt_payload(
            &URL_SAFE_NO_PAD.encode(&receiver_public),
            &URL_SAFE_NO_PAD.encode(auth),
            plaintext,
        )
        .unwrap();

        // 头必须是 salt || rs || idlen || as_public(65)
        assert!(encrypted.len() > 86);
        assert_eq!(encrypted[20], 65, "keyid 长度应为 65（未压缩点）");

        // 用接收方私钥解密（ece 的解密路径需要 import_key_pair，这里直接用
        // 我们自己持有私钥做一次独立的 ECDH+HKDF+GCM 解密校验）。
        let as_public = &encrypted[21..86];
        let salt = &encrypted[0..16];
        let ecdh = {
            let remote = crypto.import_public_key(as_public).unwrap();
            crypto
                .compute_ecdh_secret(remote.as_ref(), receiver.as_ref())
                .unwrap()
        };
        let mut key_info = b"WebPush: info\0".to_vec();
        key_info.extend_from_slice(&receiver_public);
        key_info.extend_from_slice(as_public);
        let ikm = hkdf_sha256(&auth, &ecdh, &[&key_info], 32).unwrap();
        let cek = hkdf_sha256(salt, &ikm, &[b"Content-Encoding: aes128gcm\0"], 16).unwrap();
        let nonce = hkdf_sha256(salt, &ikm, &[b"Content-Encoding: nonce\0"], 12).unwrap();

        let unbound = ring::aead::UnboundKey::new(&ring::aead::AES_128_GCM, &cek).unwrap();
        let opening = ring::aead::LessSafeKey::new(unbound);
        let mut body = encrypted[86..].to_vec();
        let nonce = ring::aead::Nonce::try_assume_unique_for_key(&nonce).unwrap();
        let decrypted = opening
            .open_in_place(nonce, ring::aead::Aad::empty(), &mut body)
            .expect("解密必须成功");
        // RFC 8188 的填充格式是「明文 || 0x02 || 零填充」：分隔符在明文之后，
        // 而填充可以跟在分隔符之后（ece 会补齐到 128 字节块），因此分隔符不一定
        // 是最后一个字节。
        assert_eq!(
            decrypted[..plaintext.len()],
            plaintext[..],
            "解密结果的前缀必须等于原始明文"
        );
        assert_eq!(
            decrypted[plaintext.len()],
            0x02,
            "明文之后必须紧跟 0x02 填充分隔符"
        );
        assert!(
            decrypted[plaintext.len() + 1..]
                .iter()
                .all(|byte| *byte == 0),
            "分隔符之后只能是零填充"
        );
    }

    /// 非法输入必须被拒绝，而不是产出垃圾载荷。
    #[test]
    fn invalid_inputs_are_rejected() {
        assert!(encrypt_payload("not base64!!", "AAAA", b"x").is_err());
        // auth secret 长度必须恰好 16 字节
        assert!(encrypt_payload(
            &URL_SAFE_NO_PAD.encode([4u8; 65]),
            &URL_SAFE_NO_PAD.encode([1u8; 8]),
            b"x"
        )
        .is_err());
        // 不在 P-256 曲线上的点必须被拒绝（RFC 8291 §7 要求）
        let mut bogus = vec![4u8; 65];
        bogus[0] = 0x04;
        assert!(encrypt_payload(
            &URL_SAFE_NO_PAD.encode(&bogus),
            &URL_SAFE_NO_PAD.encode([1u8; 16]),
            b"x"
        )
        .is_err());
    }

    // ── VAPID ─────────────────────────────────────────────────────────

    /// VAPID 的 `aud` 必须严格是端点的 origin。
    #[test]
    fn endpoint_origin_is_scheme_host_port() {
        assert_eq!(
            endpoint_origin("https://fcm.googleapis.com/fcm/send/abc123").unwrap(),
            "https://fcm.googleapis.com"
        );
        assert_eq!(
            endpoint_origin("https://push.example.net:8443/x/y").unwrap(),
            "https://push.example.net:8443"
        );
        // 非 https 必须拒绝：VAPID 只在 TLS 上有意义
        assert!(endpoint_origin("http://push.example.net/x").is_err());
        assert!(endpoint_origin("not a url").is_err());
    }

    /// `sub` 的归一化：裸邮箱补 `mailto:`，非法值退化到保留域。
    #[test]
    fn vapid_subject_is_normalized() {
        assert_eq!(normalize_subject("mailto:a@b.com"), "mailto:a@b.com");
        assert_eq!(normalize_subject("a@b.com"), "mailto:a@b.com");
        assert_eq!(normalize_subject("https://ex.com"), "https://ex.com");
        assert_eq!(normalize_subject("垃圾值"), "mailto:admin@example.invalid");
        assert_eq!(normalize_subject("   "), "mailto:admin@example.invalid");
    }

    /// VAPID JWT 必须是三段式、可被自己的公钥验证（这正是推送服务的校验动作），
    /// 且签名是 64 字节原始 r||s（不是 DER）。
    #[test]
    fn vapid_jwt_signature_verifies_against_its_own_public_key() {
        use p256::ecdsa::signature::Verifier;

        // 生成一对 VAPID 密钥，模拟设置页产生的内容
        let secret = p256::SecretKey::random(&mut p256::elliptic_curve::rand_core::OsRng);
        let public = secret.public_key().to_encoded_point(false);
        let private_b64 = URL_SAFE_NO_PAD.encode(secret.to_bytes());
        let public_b64 = URL_SAFE_NO_PAD.encode(public.as_bytes());

        let endpoint = "https://fcm.googleapis.com/fcm/send/xyz";
        let header =
            vapid_authorization(&private_b64, &public_b64, endpoint, "admin@example.com").unwrap();

        // 形如 vapid t=<jwt>, k=<key>
        let jwt = header
            .strip_prefix("vapid t=")
            .and_then(|rest| rest.split(", k=").next())
            .expect("Authorization 头格式应为 vapid t=..., k=...");
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT 应为 header.payload.signature 三段");

        // 签名长度必须是 64 字节（ES256 的 JWS 形式）
        let signature = b64(parts[2]);
        assert_eq!(
            signature.len(),
            64,
            "ES256 签名必须是 64 字节 r||s，不能是 DER 编码"
        );

        // 用公钥验证签名 —— 与推送服务的校验完全一致
        let signing_key = p256::ecdsa::SigningKey::from_slice(&secret.to_bytes()).unwrap();
        let verifying = signing_key.verifying_key();
        let signature = p256::ecdsa::Signature::from_slice(&signature).expect("签名应可解析");
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        assert!(
            verifying
                .verify(signing_input.as_bytes(), &signature)
                .is_ok(),
            "VAPID 签名必须能用对应公钥验证通过"
        );

        // header 与 claims 的内容
        let header_json: serde_json::Value = serde_json::from_slice(&b64(parts[0])).unwrap();
        assert_eq!(header_json["alg"], "ES256");
        assert_eq!(header_json["typ"], "JWT");

        let claims: serde_json::Value = serde_json::from_slice(&b64(parts[1])).unwrap();
        assert_eq!(claims["aud"], "https://fcm.googleapis.com");
        assert_eq!(claims["sub"], "mailto:admin@example.com");
        let exp = claims["exp"].as_u64().unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(
            exp > now && exp <= now + 24 * 60 * 60,
            "VAPID exp 必须在未来且不超过规范的 24 小时上限"
        );
    }

    /// 非法 VAPID 私钥/公钥必须被拒绝并给出可操作的错误。
    #[test]
    fn vapid_rejects_malformed_keys() {
        let secret = p256::SecretKey::random(&mut p256::elliptic_curve::rand_core::OsRng);
        let public_b64 =
            URL_SAFE_NO_PAD.encode(secret.public_key().to_encoded_point(false).as_bytes());

        assert!(vapid_authorization("!!!", &public_b64, "https://a.com/x", "a@b.c").is_err());
        // 私钥长度错误（全零不是合法标量）
        assert!(vapid_authorization(
            &URL_SAFE_NO_PAD.encode([0u8; 32]),
            &public_b64,
            "https://a.com/x",
            "a@b.c"
        )
        .is_err());
        // 公钥不是未压缩点
        let private_b64 = URL_SAFE_NO_PAD.encode(secret.to_bytes());
        assert!(vapid_authorization(
            &private_b64,
            &URL_SAFE_NO_PAD.encode([1u8; 33]),
            "https://a.com/x",
            "a@b.c"
        )
        .is_err());
    }
}
