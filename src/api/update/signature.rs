//! 自更新产物的**真实性**校验（Ed25519 / minisign 兼容）。
//!
//! # 为什么需要它
//!
//! 更新器此前只校验 SHA-256，而校验和与压缩包来自**同一个 Release**——攻击者一旦
//! 拿到仓库/账号控制权（或打破 TLS 信任），就能同时提供被篡改的载荷和与之匹配的
//! 校验和，SHA-256 在此**不提供任何真实性保证**。载荷随后会被解包并覆盖运行中的
//! 二进制，等价于以服务身份执行任意代码。
//!
//! 真实性只能来自一个**不经过同一下载渠道**的信任根：把公钥编译进二进制，
//! 用它校验发布方用私钥签出的分离签名。
//!
//! # 为什么选 minisign 格式
//!
//! - 生态标准：`minisign` / `signify` 广泛用于二进制自更新的签名；
//! - 实现极简且**可审计**：签名体固定 74 字节（2 字节算法 + 8 字节 key id +
//!   64 字节 Ed25519 签名），用 `ring` 的 Ed25519 验签即可，无需引入
//!   OpenSSL 或 sigstore 这类重依赖；
//! - 发布方一条命令即可签名：`minisign -S -s key.sec -m asset.tar.gz`。
//!
//! # 启用方式
//!
//! 默认**不启用**（保持与旧版本一致的行为，避免升级后突然无法自动更新）。
//! 启用有两种方式：
//!
//! 1. 编译期：设置 `SELF_UPDATE_PUBLIC_KEY` 环境变量构建，公钥会被 `option_env!`
//!    固化进二进制——**推荐**，因为运行时环境变量可以被能改配置的攻击者一并改掉；
//! 2. 运行时：设置同名环境变量。
//!
//! 一旦配置了公钥，签名就是**强制**的：缺少签名、签名格式错误或验签失败都会
//! 中止更新。未配置时会在更新流程里打一条 WARN，明确说明当前只有完整性校验。
//!
//! 发布方需要在 Release 里额外上传 `<asset>.minisig`。缺少该文件时更新会因为
//! 「签名缺失」而失败——这是刻意的失败关闭行为。

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ring::signature;

/// minisign 签名体的固定长度：2（算法）+ 8（key id）+ 64（Ed25519 签名）。
const SIGNATURE_BODY_LEN: usize = 74;
/// 签名体的前两字节（minisign 规范，与 `minisign-verify` 参考实现一致）：
///
/// - `ED`（0x45 0x44）= **预哈希**：对内容的 BLAKE2b-512 摘要签名，
///   这是 `minisign -S` 的默认模式，也是 release.yml 产出的模式；
/// - `Ed`（0x45 0x64）= 旧式**直接**签名（`-l` legacy），对原始内容签名。
///
/// 两者都支持。注意大小写不能弄反：反了会让每一个由官方 minisign 生成的
/// 签名都验证失败（用原始字节去验一个对摘要做的签名），而单元测试若用同一套
/// 错误假设自造签名则完全发现不了——所以下面有一条用官方 minisign 固件的测试。
const ALG_PREHASHED: &[u8; 2] = b"ED";
const ALG_DIRECT: &[u8; 2] = b"Ed";

/// 从环境变量读取配置的公钥（base64）。
///
/// 优先使用**编译期**固化的值：运行时环境变量可以被能修改服务配置的攻击者一并
/// 改掉，而编译进二进制的公钥不能。
fn configured_public_key() -> Option<&'static str> {
    option_env!("SELF_UPDATE_PUBLIC_KEY")
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            // 运行时值必须泄漏成 'static 才能与上面的分支统一返回类型；
            // 一次性的读取代价可忽略。
            std::env::var("SELF_UPDATE_PUBLIC_KEY")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .map(|value| &*Box::leak(value.into_boxed_str()))
        })
}

/// 是否已配置签名公钥（即签名校验是否处于强制状态）。
pub fn signature_verification_enabled() -> bool {
    configured_public_key().is_some()
}

/// 校验 `content` 的 minisign 分离签名。
///
/// `signature_file` 是 `.minisig` 文件的完整内容（两行注释 + 一行 base64 签名体，
/// 或仅 base64 一行）。
pub fn verify_minisign(content: &[u8], signature_file: &str) -> Result<(), String> {
    let public_key_b64 = configured_public_key()
        .ok_or_else(|| "未配置 SELF_UPDATE_PUBLIC_KEY，无法校验签名".to_string())?;
    verify_minisign_with_key(public_key_b64, content, signature_file)
}

/// 用显式给定的公钥校验，便于测试与离线校验工具复用。
pub fn verify_minisign_with_key(
    public_key_b64: &str,
    content: &[u8],
    signature_file: &str,
) -> Result<(), String> {
    let (expected_key_id, public_key) = decode_public_key(public_key_b64)?;
    let (algorithm, key_id, signature_bytes) = parse_signature_file(signature_file)?;
    // key id 由 minisign 随机生成并同时写进公钥与签名；不一致说明签名出自另一把
    // 密钥，直接拒绝，避免拿错公钥时得到一条含糊的「验签失败」。
    // 裸 32 字节公钥没有 key id，此时跳过比对。
    if let Some(expected) = expected_key_id {
        if expected != key_id {
            return Err(format!(
                "minisign 签名的 key id ({}) 与配置公钥的 key id ({}) 不一致",
                hex(&key_id),
                hex(&expected)
            ));
        }
    }

    let verifier = signature::UnparsedPublicKey::new(&signature::ED25519, &public_key);
    // minisign 的默认模式是「预哈希」：对内容的 BLAKE2b-512 摘要签名。
    // 两字节算法的第二字节大小写区分两种模式（`ED` = prehashed，`Ed` = legacy/direct）。
    let message: Vec<u8> = if algorithm == *ALG_PREHASHED {
        blake2b512(content)
    } else {
        content.to_vec()
    };
    verifier
        .verify(&message, &signature_bytes)
        .map_err(|_| "自更新产物签名校验失败（签名与公钥不匹配或内容被篡改）".to_string())
}

/// 解码 minisign 公钥。
///
/// 接受两种形式：
/// - base64 的 42 字节二进制公钥（minisign `.pub` 文件去掉注释行后的内容）：
///   2 字节算法 + 8 字节 key id + 32 字节 Ed25519 公钥；
/// - 直接的 32 字节 base64 公钥。
fn decode_public_key(value: &str) -> Result<(Option<[u8; 8]>, Vec<u8>), String> {
    // 允许把整个 .pub 文件内容贴进来：取第一个能解码出正确长度的行。
    for line in value.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("untrusted comment:") {
            continue;
        }
        let Ok(bytes) = STANDARD.decode(line) else {
            continue;
        };
        if bytes.len() == 42 {
            // 2 字节算法 + 8 字节 key id + 32 字节公钥
            if &bytes[0..2] != ALG_DIRECT {
                return Err(
                    "SELF_UPDATE_PUBLIC_KEY 的算法标记不是 Ed25519 minisign 公钥".to_string(),
                );
            }
            let mut key_id = [0u8; 8];
            key_id.copy_from_slice(&bytes[2..10]);
            return Ok((Some(key_id), bytes[10..42].to_vec()));
        }
        if bytes.len() == 32 {
            return Ok((None, bytes));
        }
    }
    Err(
        "SELF_UPDATE_PUBLIC_KEY 不是合法的 minisign 公钥（期望 42 字节 base64，或 32 字节裸公钥）"
            .to_string(),
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 解析 `.minisig` 文件，返回（算法标记, key id, 64 字节签名）。
///
/// 文件格式（minisign）：
/// ```text
/// untrusted comment: <任意>
/// <base64 签名体 74 字节>
/// trusted comment: <任意>
/// <base64 全局签名 64 字节：对「签名体 || trusted comment」的 Ed25519 签名>
/// ```
/// 只取第一行能解出 74 字节的 base64；全局签名与 trusted comment 不参与
/// 校验（它们只保护评论文本，本项目不使用评论内容）。
/// 解析出的签名体：算法标记、key id、64 字节 Ed25519 签名。
type ParsedSignature = ([u8; 2], [u8; 8], Vec<u8>);

fn parse_signature_file(content: &str) -> Result<ParsedSignature, String> {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with("untrusted comment:")
            || line.starts_with("trusted comment:")
        {
            continue;
        }
        let Ok(bytes) = STANDARD.decode(line) else {
            continue;
        };
        if bytes.len() != SIGNATURE_BODY_LEN {
            return Err(format!(
                "minisign 签名体长度异常（期望 {} 字节，实际 {}）",
                SIGNATURE_BODY_LEN,
                bytes.len()
            ));
        }
        let algorithm = [bytes[0], bytes[1]];
        if algorithm != *ALG_PREHASHED && algorithm != *ALG_DIRECT {
            return Err("minisign 签名算法不是 Ed25519（仅支持 ED/Ed 模式）".to_string());
        }
        let mut key_id = [0u8; 8];
        key_id.copy_from_slice(&bytes[2..10]);
        return Ok((algorithm, key_id, bytes[10..].to_vec()));
    }
    Err("签名文件里没有可解析的 minisign 签名行".to_string())
}

/// BLAKE2b-512，用于 minisign 的预哈希模式。
///
/// `ring::digest` 不提供 BLAKE2b，因此这里给出一个最小实现：BLAKE2b 的 IV 与
/// sigma 置换是规范固定的常量，实现不长且可对照 RFC 7693 逐行审阅。
/// 用官方测试向量（RFC 7693 附录 A）验证。
fn blake2b512(input: &[u8]) -> Vec<u8> {
    /// BLAKE2b 的 IV（RFC 7693 §2.6）。
    const IV: [u64; 8] = [
        0x6a09e667f3bcc908,
        0xbb67ae8584caa73b,
        0x3c6ef372fe94f82b,
        0xa54ff53a5f1d36f1,
        0x510e527fade682d1,
        0x9b05688c2b3e6c1f,
        0x1f83d9abfb41bd6b,
        0x5be0cd19137e2179,
    ];
    const BLOCK: usize = 128;

    // 参数块：digest 长度 64、无密钥、fanout=1、depth=1。
    let mut state = IV;
    state[0] ^= 0x0101_0000 ^ 64;

    let mut counter: u128 = 0;
    let mut offset = 0usize;
    // 除最后一块之外，所有整块都用 last=false 压缩。
    // 条件是「剩余 > BLOCK」而不是「>=」：正好整块时，那一块本身就要作为带 last
    // 标志的最终块处理——这是 BLAKE2 与多数哈希不同的地方，也是最容易写错的一处。
    while input.len().saturating_sub(offset) > BLOCK {
        counter += BLOCK as u128;
        compress(&mut state, &input[offset..offset + BLOCK], counter, false);
        offset += BLOCK;
    }

    // 最终块：余数（可能为 0）零填充到 128 字节。
    let remainder = &input[offset..];
    debug_assert!(remainder.len() <= BLOCK);
    let mut final_block = [0u8; BLOCK];
    final_block[..remainder.len()].copy_from_slice(remainder);
    counter += remainder.len() as u128;
    compress(&mut state, &final_block, counter, true);

    let mut out = Vec::with_capacity(64);
    for word in state {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out
}

/// BLAKE2b 的压缩函数 F（RFC 7693 §3.2）。
fn compress(state: &mut [u64; 8], block: &[u8], counter: u128, last: bool) {
    const IV: [u64; 8] = [
        0x6a09e667f3bcc908,
        0xbb67ae8584caa73b,
        0x3c6ef372fe94f82b,
        0xa54ff53a5f1d36f1,
        0x510e527fade682d1,
        0x9b05688c2b3e6c1f,
        0x1f83d9abfb41bd6b,
        0x5be0cd19137e2179,
    ];
    const SIGMA: [[usize; 16]; 12] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
        [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
        [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
        [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
        [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
        [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
        [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
        [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
        [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    ];

    debug_assert_eq!(block.len(), 128);
    let mut m = [0u64; 16];
    for (index, word) in m.iter_mut().enumerate() {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&block[index * 8..index * 8 + 8]);
        *word = u64::from_le_bytes(bytes);
    }

    let mut v = [0u64; 16];
    v[..8].copy_from_slice(state);
    v[8..].copy_from_slice(&IV);
    v[12] ^= counter as u64;
    v[13] ^= (counter >> 64) as u64;
    if last {
        v[14] = !v[14];
    }

    for round in SIGMA {
        mix(&mut v, 0, 4, 8, 12, m[round[0]], m[round[1]]);
        mix(&mut v, 1, 5, 9, 13, m[round[2]], m[round[3]]);
        mix(&mut v, 2, 6, 10, 14, m[round[4]], m[round[5]]);
        mix(&mut v, 3, 7, 11, 15, m[round[6]], m[round[7]]);
        mix(&mut v, 0, 5, 10, 15, m[round[8]], m[round[9]]);
        mix(&mut v, 1, 6, 11, 12, m[round[10]], m[round[11]]);
        mix(&mut v, 2, 7, 8, 13, m[round[12]], m[round[13]]);
        mix(&mut v, 3, 4, 9, 14, m[round[14]], m[round[15]]);
    }

    for index in 0..8 {
        state[index] ^= v[index] ^ v[index + 8];
    }
}

#[inline]
fn mix(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
    v[d] = (v[d] ^ v[a]).rotate_right(32);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(24);
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
    v[d] = (v[d] ^ v[a]).rotate_right(16);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(63);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(bytes: &[u8]) -> String {
        STANDARD.encode(bytes)
    }

    /// RFC 7693 附录 A：BLAKE2b-512("abc")。
    ///
    /// 自实现 BLAKE2b 的唯一理由是 `ring::digest` 不提供它，而 minisign 的默认
    /// 预哈希模式需要它。用官方向量锁住实现正确性。
    #[test]
    fn blake2b512_matches_rfc7693_vector() {
        let digest = blake2b512(b"abc");
        let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(
            hex,
            "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d1\
             7d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923"
        );
    }

    /// 空输入与跨块输入（BLAKE2b 的 128 字节块边界最容易写错）。
    #[test]
    fn blake2b512_handles_empty_and_multiblock_inputs() {
        let empty = blake2b512(b"");
        let hex: String = empty.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(
            hex,
            "786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419\
             d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce"
        );

        // 128 字节（正好一整块）与 129 字节（跨块）都必须稳定且互不相同
        let one_block = blake2b512(&[0u8; 128]);
        let two_blocks = blake2b512(&[0u8; 129]);
        assert_ne!(one_block, two_blocks);
        assert_eq!(one_block.len(), 64);
        // 可复现
        assert_eq!(one_block, blake2b512(&[0u8; 128]));
    }

    /// 用 `ring` 现场生成密钥对，构造一个符合 minisign 布局的签名并验证通过。
    #[test]
    fn minisign_signature_verifies_for_matching_key() {
        use ring::rand::SystemRandom;
        use ring::signature::{Ed25519KeyPair, KeyPair};

        let rng = SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let keypair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let public = keypair.public_key().as_ref().to_vec();

        let content = b"the release archive bytes";
        let digest = blake2b512(content);
        let signature = keypair.sign(&digest);

        // 组装 minisign 风格的公钥（42 字节，公钥的算法标记固定为 `Ed`）
        // 与签名文件（74 字节）
        let mut pub_blob = Vec::new();
        pub_blob.extend_from_slice(ALG_DIRECT);
        pub_blob.extend_from_slice(&[1u8; 8]); // key id
        pub_blob.extend_from_slice(&public);

        let mut sig_blob = Vec::new();
        sig_blob.extend_from_slice(ALG_PREHASHED);
        sig_blob.extend_from_slice(&[1u8; 8]); // key id
        sig_blob.extend_from_slice(signature.as_ref());

        let pub_text = format!(
            "untrusted comment: minisign public key\n{}\n",
            b64(&pub_blob)
        );
        let sig_text = format!(
            "untrusted comment: signature from minisign secret key\n{}\ntrusted comment: test\n{}\n",
            b64(&sig_blob),
            b64(b"trusted comment signature")
        );

        let (key_id, key) = decode_public_key(&pub_text).unwrap();
        assert_eq!(key, public, "42 字节公钥应解出 32 字节 Ed25519 公钥");
        assert_eq!(key_id, Some([1u8; 8]));

        let (algorithm, sig_key_id, sig_bytes) = parse_signature_file(&sig_text).unwrap();
        assert_eq!(algorithm, *ALG_PREHASHED);
        assert_eq!(sig_key_id, [1u8; 8]);
        assert_eq!(sig_bytes.len(), 64);

        // 走完整校验入口（含 key id 比对）
        verify_minisign_with_key(&pub_text, content, &sig_text)
            .expect("完整校验入口必须接受匹配的签名");
        // key id 不一致必须被拒绝，即使签名本身有效
        let mut other_pub = Vec::new();
        other_pub.extend_from_slice(ALG_DIRECT);
        other_pub.extend_from_slice(&[9u8; 8]);
        other_pub.extend_from_slice(&public);
        let error = verify_minisign_with_key(&b64(&other_pub), content, &sig_text).unwrap_err();
        assert!(error.contains("key id"), "应报告 key id 不一致: {error}");

        let verifier = signature::UnparsedPublicKey::new(&signature::ED25519, &key);
        assert!(
            verifier.verify(&digest, &sig_bytes).is_ok(),
            "匹配的签名必须验证通过"
        );
        // 内容被改一个字节就必须失败
        let tampered = blake2b512(b"the release archive byteS");
        assert!(
            verifier.verify(&tampered, &sig_bytes).is_err(),
            "内容被篡改后必须验证失败"
        );
    }

    /// **官方 minisign 固件**：公钥与预哈希签名均由真实 `minisign` 生成
    /// （取自 minisign-verify 参考实现的测试向量，对内容 `test` 签名）。
    ///
    /// 这条测试是防止「`ED`/`Ed` 标记弄反」的唯一护栏——用 `ring` 自造签名的
    /// 测试会与实现共享同一套假设，无论正反都能通过。
    #[test]
    fn official_minisign_prehashed_fixture_verifies() {
        const PUBLIC_KEY: &str = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
        const SIGNATURE: &str = "untrusted comment: signature from minisign secret key
RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=
trusted comment: timestamp:1556193335	file:test
y/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg==
";

        let (algorithm, _, _) = parse_signature_file(SIGNATURE).unwrap();
        assert_eq!(
            algorithm, *ALG_PREHASHED,
            "minisign -S 默认产出预哈希（ED）签名"
        );

        verify_minisign_with_key(PUBLIC_KEY, b"test", SIGNATURE)
            .expect("官方 minisign 预哈希签名必须验证通过");
        assert!(
            verify_minisign_with_key(PUBLIC_KEY, b"tesT", SIGNATURE).is_err(),
            "内容被改后必须失败"
        );
        // 整份 .pub 文件内容也应可用
        let pub_file = format!(
            "untrusted comment: minisign public key
{PUBLIC_KEY}
"
        );
        verify_minisign_with_key(&pub_file, b"test", SIGNATURE).unwrap();
    }

    /// **官方 minisign 固件**（legacy `Ed` 模式，`minisign -l`）：直接对内容签名。
    #[test]
    fn official_minisign_legacy_fixture_verifies() {
        const PUBLIC_KEY: &str = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
        const SIGNATURE: &str = "untrusted comment: signature from minisign secret key
RWQf6LRCGA9i59SLOFxz6NxvASXDJeRtuZykwQepbDEGt87ig1BNpWaVWuNrm73YiIiJbq71Wi+dP9eKL8OC351vwIasSSbXxwA=
trusted comment: timestamp:1555779966	file:test
";

        let (algorithm, _, _) = parse_signature_file(SIGNATURE).unwrap();
        assert_eq!(algorithm, *ALG_DIRECT);
        verify_minisign_with_key(PUBLIC_KEY, b"test", SIGNATURE)
            .expect("官方 minisign legacy 签名必须验证通过");
    }

    /// 直接签名模式（`Ed`）不预哈希。
    #[test]
    fn direct_signature_mode_does_not_prehash() {
        use ring::rand::SystemRandom;
        use ring::signature::{Ed25519KeyPair, KeyPair};

        let rng = SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let keypair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();

        let content = b"raw content";
        let signature = keypair.sign(content);

        let mut sig_blob = Vec::new();
        sig_blob.extend_from_slice(ALG_DIRECT);
        sig_blob.extend_from_slice(&[2u8; 8]);
        sig_blob.extend_from_slice(signature.as_ref());
        let sig_text = format!("untrusted comment: x\n{}\n", b64(&sig_blob));

        let (algorithm, _, sig_bytes) = parse_signature_file(&sig_text).unwrap();
        assert_eq!(algorithm, *ALG_DIRECT);
        let verifier =
            signature::UnparsedPublicKey::new(&signature::ED25519, keypair.public_key().as_ref());
        assert!(verifier.verify(content, &sig_bytes).is_ok());
    }

    /// 非法输入必须被明确拒绝，不能静默放行。
    #[test]
    fn malformed_signatures_and_keys_are_rejected() {
        assert!(decode_public_key("not base64!!").is_err());
        assert!(decode_public_key("").is_err());
        // 长度不对（20 字节）
        assert!(decode_public_key(&b64(&[0u8; 20])).is_err());

        assert!(parse_signature_file("").is_err());
        assert!(parse_signature_file("untrusted comment: only a comment\n").is_err());
        // 签名体长度不对
        assert!(parse_signature_file(&format!("{}\n", b64(&[0u8; 10]))).is_err());
        // 算法标记不对
        let mut bad = vec![b'X', b'X'];
        bad.extend_from_slice(&[0u8; 72]);
        assert!(parse_signature_file(&format!("{}\n", b64(&bad))).is_err());
    }

    /// 未配置公钥时签名校验不可用，且必须**显式**报告而不是默认通过。
    #[test]
    fn verify_reports_error_when_no_public_key_configured() {
        // 该测试运行在未设置 SELF_UPDATE_PUBLIC_KEY 的环境里（CI 不设置）。
        if signature_verification_enabled() {
            return;
        }
        let error = verify_minisign(b"content", "untrusted comment: x\nAAAA\n").unwrap_err();
        assert!(
            error.contains("SELF_UPDATE_PUBLIC_KEY"),
            "错误信息应指明缺少哪个配置: {error}"
        );
    }
}
