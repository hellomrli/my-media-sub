//! 稳定标识符派生：截断 SHA-256。
//!
//! 这些标识符（订阅 ID、Job 幂等键、Telegram 回调 ID、导入指纹、搜索命中 ID）
//! 都不需要密码学强度的抗碰撞，但它们此前用的是 **MD5**——一个已被攻破的哈希，
//! 导致每次安全审计都要额外解释一遍「这里的 MD5 是否可被利用」。
//!
//! 改用截断 SHA-256 后：
//!
//! - **长度不变**：仍然是 16 字节 / 32 个十六进制字符，因此调用点的 `&id[..12]`
//!   这类截断与格式假设全部继续成立；
//! - **既有数据无需迁移**：订阅 ID 只在创建时生成一次并持久化，旧记录保留自己的
//!   ID；只有升级瞬间未完成的幂等去重可能失效一次（可接受）；
//! - 项目从此不再依赖 `md5` crate。
//!
//! 单独作为**无依赖叶子模块**（只用 `ring`）存在，因为 `src/jobs/model.rs` 会被
//! `tests/real_data_compat.rs` 以 `#[path]` 直接编译，那条路径上没有 `crate::utils`
//! （`utils` 依赖 `AppError`、`libc` 等一堆东西）。

/// 稳定的 16 字节摘要（SHA-256 截断）。
pub fn stable_id_bytes(material: impl AsRef<[u8]>) -> [u8; 16] {
    let digest = ring::digest::digest(&ring::digest::SHA256, material.as_ref());
    let mut out = [0u8; 16];
    out.copy_from_slice(&digest.as_ref()[..16]);
    out
}

/// 同 [`stable_id_bytes`]，输出 32 位小写十六进制字符串。
pub fn stable_id(material: impl AsRef<[u8]>) -> String {
    stable_id_bytes(material)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_stable_and_hex() {
        let first = stable_id("https://pan.quark.cn/s/abc:庆余年");
        let second = stable_id("https://pan.quark.cn/s/abc:庆余年");
        assert_eq!(first, second, "同一输入必须得到同一标识符");
        assert_eq!(first.len(), 32, "长度必须与旧 MD5 一致（32 位十六进制）");
        assert!(first.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn identifiers_differ_for_different_inputs() {
        assert_ne!(stable_id("a"), stable_id("b"));
        assert_ne!(stable_id("url:title"), stable_id("url:title2"));
    }

    /// 不能与旧 MD5 值相同，否则说明替换没生效。
    #[test]
    fn is_not_the_legacy_md5_value() {
        // md5("a") == 0cc175b9c0f1b6a831c399e269772661
        assert_ne!(stable_id("a"), "0cc175b9c0f1b6a831c399e269772661");
    }

    #[test]
    fn byte_form_matches_string_form() {
        let bytes = stable_id_bytes("material");
        let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(hex, stable_id("material"));
    }

    /// 重试抖动依赖首字节，必须真的有变化范围（否则所有重试都同时发生）。
    #[test]
    fn byte_form_varies_across_inputs() {
        let distinct: std::collections::HashSet<u8> = (0..64)
            .map(|attempt| stable_id_bytes(format!("job:{attempt}"))[0])
            .collect();
        assert!(distinct.len() > 8, "首字节分布过于集中: {distinct:?}");
    }
}
