//! 文件名可移植化：跨 Linux / Windows / 常见 NAS 文件系统的安全文件名。
//!
//! 单独作为**无依赖叶子模块**存在，原因是三处都需要它，且各自处在不同层：
//!
//! - `services::transfer_rule`（重命名模板产出）；
//! - `clients::aria2`（提交给下载器的 `out` 选项，最后一道防线）；
//! - `tests/subscription_flow.rs`（以 `#[path]` 直接编译本文件做规则测试）。
//!
//! 放进 `utils` 会让最后一个场景拉进 `AppError`/`libc` 等一堆无关依赖，
//! 所以这里保持零依赖。

/// Produce a portable filename for Linux, Windows and common NAS filesystems.
///
/// 清掉控制字符与 `/ \ : * ? " < > |`，去掉结尾的点与空格（Windows 不允许），
/// 空结果回退为 `unnamed`，并截断到 240 字符。
pub fn portable_filename(name: &str) -> String {
    let mut output = name
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            {
                '_'
            } else {
                ch
            }
        })
        .collect::<String>();
    output = output.trim().trim_end_matches(['.', ' ']).to_string();
    if output.is_empty() {
        output = "unnamed".to_string();
    }
    if output.chars().count() > 240 {
        output = output.chars().take(240).collect();
    }
    output
}

/// 判断清洗后是否仍然是一个"有意义的文件名"：非空、且不是 `.` / `..`。
///
/// 供调用方在提交给外部系统前做显式拒绝，避免把目录占位符当成文件名。
pub fn is_safe_filename(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty() && trimmed != "." && trimmed != ".."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_path_separators_and_reserved_characters() {
        assert_eq!(
            portable_filename("a/b\\c:d*e?f\"g<h>i|j"),
            "a_b_c_d_e_f_g_h_i_j"
        );
        assert_eq!(portable_filename("bad:name?.mkv"), "bad_name_.mkv");
    }

    #[test]
    fn strips_trailing_dots_and_spaces() {
        assert_eq!(portable_filename("name...  "), "name");
        assert_eq!(portable_filename("  padded.mkv  "), "padded.mkv");
    }

    #[test]
    fn never_returns_empty_and_truncates_long_names() {
        assert_eq!(portable_filename("   "), "unnamed");
        assert_eq!(portable_filename("..."), "unnamed");
        let long = "字".repeat(300);
        assert_eq!(portable_filename(&long).chars().count(), 240);
    }

    /// 清洗后不得再含路径穿越片段。
    #[test]
    fn removes_traversal_sequences() {
        assert_eq!(portable_filename("../../etc/passwd"), ".._.._etc_passwd");
        assert!(!portable_filename("../../etc/passwd").contains('/'));
    }

    #[test]
    fn rejects_directory_placeholders() {
        assert!(is_safe_filename("movie.mkv"));
        assert!(!is_safe_filename("."));
        assert!(!is_safe_filename(".."));
        assert!(!is_safe_filename("   "));
    }
}
