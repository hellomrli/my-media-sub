use super::*;

pub(super) async fn download_asset(url: &str, path: &Path, expected_size: u64) -> Result<()> {
    set_update_progress(10, "download", "正在连接 Release 下载地址");
    ensure_allowed_update_url(url)?;
    // 发布压缩包通常 10MB 以上，跨境下载经常超过 30 秒的总超时；
    // 大文件下载使用自更新专用客户端（300 秒超时 + 逐跳校验重定向主机）。
    let mut response = http_pool::update_client()
        .get(url)
        .header(reqwest::header::USER_AGENT, "my-media-sub-self-update")
        .send()
        .await?
        .error_for_status()?;

    let fallback_total_bytes = (expected_size > 0).then_some(expected_size);
    let total_bytes = response.content_length().or(fallback_total_bytes);
    if let Some(total) = total_bytes {
        if total
            > expected_size
                .saturating_mul(MAX_UPDATE_PACKAGE_BYTES)
                .max(64 * 1024 * 1024)
        {
            return Err(AppError::Validation(
                "升级包体积异常，已取消下载".to_string(),
            ));
        }
    }
    let max_bytes = expected_size
        .saturating_mul(MAX_UPDATE_PACKAGE_BYTES)
        .max(64 * 1024 * 1024);
    let mut downloaded_bytes = 0u64;
    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(|e| AppError::Internal(format!("创建升级包文件失败: {}", e)))?;

    while let Some(chunk) = response.chunk().await? {
        downloaded_bytes += chunk.len() as u64;
        if downloaded_bytes > max_bytes {
            return Err(AppError::Validation(
                "升级包体积异常，已取消下载".to_string(),
            ));
        }
        file.write_all(&chunk)
            .await
            .map_err(|e| AppError::Internal(format!("写入升级包失败: {}", e)))?;
        set_download_progress(downloaded_bytes, total_bytes);
    }
    file.flush()
        .await
        .map_err(|e| AppError::Internal(format!("刷新升级包文件失败: {}", e)))?;
    file.sync_all()
        .await
        .map_err(|e| AppError::Internal(format!("同步升级包文件失败: {}", e)))?;
    set_download_progress(downloaded_bytes, total_bytes);
    Ok(())
}

pub(super) async fn download_asset_bytes(url: &str) -> Result<Vec<u8>> {
    ensure_allowed_update_url(url)?;
    let response = http_pool::update_client()
        .get(url)
        .header(reqwest::header::USER_AGENT, "my-media-sub-self-update")
        .send()
        .await?
        .error_for_status()?;
    Ok(response.bytes().await?.to_vec())
}

/// 下载前校验 URL 的主机在白名单内。
///
/// `browser_download_url` 直接来自 GitHub API 的 JSON，属于**外部输入**；旧实现
/// 逐字采纳它且不限制重定向，因此 API 响应被篡改时可以把下载引到任意地址，
/// 而下载内容会被解包并覆盖运行中的二进制。客户端侧还有一层逐跳重定向校验
/// （`http_pool::update_client`），两处都做是为了让「初始 URL 非法」直接给出
/// 明确错误，而不是等到重定向被拒。
pub(super) fn ensure_allowed_update_url(url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|error| AppError::Validation(format!("更新下载地址无法解析: {error}")))?;
    if !http_pool::is_allowed_update_host(&parsed) {
        return Err(AppError::Validation(format!(
            "更新下载地址的域名不在允许列表内（仅限 GitHub 资产主机）: {}",
            parsed.host_str().unwrap_or("未知主机")
        )));
    }
    Ok(())
}

pub(super) async fn verify_sha256(
    path: &Path,
    asset_name: &str,
    checksum_content: &[u8],
) -> Result<()> {
    let expected = parse_sha256_checksum(checksum_content, asset_name)
        .ok_or_else(|| AppError::Validation("SHA256 校验文件格式无效".to_string()))?;
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| AppError::Internal(format!("读取升级包失败: {}", e)))?;
    let actual = digest::digest(&digest::SHA256, &bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<String>();

    if !constant_time_eq(&actual, &expected) {
        return Err(AppError::Validation("升级包 SHA256 校验失败".to_string()));
    }

    Ok(())
}

pub(super) fn parse_sha256_checksum(content: &[u8], asset_name: &str) -> Option<String> {
    let text = String::from_utf8_lossy(content);
    let mut bare_checksum = None;
    let mut bare_count = 0usize;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts = line.split_whitespace().collect::<Vec<_>>();
        let Some(checksum) = parts.iter().copied().find(|part| is_sha256_checksum(part)) else {
            continue;
        };

        if checksum_matches_asset_line(line, checksum, asset_name) {
            return Some(checksum.to_ascii_lowercase());
        }

        if parts.len() == 1 && parts[0] == checksum {
            bare_count += 1;
            bare_checksum = Some(checksum.to_ascii_lowercase());
        }
    }

    (bare_count == 1).then_some(bare_checksum?).filter(|_| {
        text.lines()
            .filter(|line| {
                let line = line.trim();
                !line.is_empty() && !line.starts_with('#')
            })
            .count()
            == 1
    })
}

pub(super) fn is_sha256_checksum(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

pub(super) fn checksum_matches_asset_line(line: &str, checksum: &str, asset_name: &str) -> bool {
    let normalized = line.replace('*', " ");
    if normalized.split_whitespace().any(|part| part == asset_name) {
        return true;
    }

    let bsd_prefix = format!("SHA256 ({asset_name}) =");
    line.starts_with(&bsd_prefix) && line.split_whitespace().last() == Some(checksum)
}

pub(super) async fn extract_archive(archive_path: &Path, output_dir: &Path) -> Result<()> {
    let archive_path = archive_path.to_path_buf();
    let output_dir = output_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        verify_archive_members(&archive_path)?;
        let output = std::process::Command::new("tar")
            .arg("-xzf")
            .arg(&archive_path)
            .arg("-C")
            .arg(&output_dir)
            // 升级包由发行流水线以非 root 构建，不保留属主/权限位可避免本地
            // 覆盖时把升级包里的 uid/可执行位原样写进运行目录。
            .arg("--no-same-owner")
            .arg("--no-same-permissions")
            .output()
            .map_err(|e| AppError::Internal(format!("执行 tar 解压失败: {}", e)))?;
        if !output.status.success() {
            return Err(AppError::Internal(format!(
                "解压升级包失败: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        ensure_extracted_inside(&output_dir)
    })
    .await
    .map_err(|e| AppError::Internal(format!("解压任务失败: {}", e)))?
}

/// 解压前校验归档成员：成员名必须为相对路径、不含 `..` 组件且非空，链接
/// 目标不得为绝对路径或包含 `..`。校验通过前不执行任何解压，防止恶意
/// 升级包把文件写出 work_dir。
pub(super) fn verify_archive_members(archive_path: &Path) -> Result<()> {
    let list = std::process::Command::new("tar")
        .arg("-tvzf")
        .arg(archive_path)
        .output()
        .map_err(|e| AppError::Internal(format!("列出升级包内容失败: {}", e)))?;
    if !list.status.success() {
        return Err(AppError::Internal(format!(
            "列出升级包内容失败: {}",
            String::from_utf8_lossy(&list.stderr)
        )));
    }
    let listing = String::from_utf8_lossy(&list.stdout);
    let mut member_count = 0usize;
    for line in listing.lines() {
        // GNU tar 冗长列表格式：`permissions owner/group size date name [-> target]`。
        // 符号链接行含 ` -> `，需额外校验链接目标不逃逸。
        let (name, target) = parse_tar_listing_line(line);
        if !is_safe_member_path(name) {
            return Err(AppError::Validation(format!(
                "升级包包含不安全路径: {:?}",
                line
            )));
        }
        if let Some(target) = target {
            if !is_safe_member_path(target) {
                return Err(AppError::Validation(format!(
                    "升级包包含逃逸链接目标: {:?}",
                    line
                )));
            }
        }
        member_count += 1;
    }
    if member_count == 0 {
        return Err(AppError::Validation("升级包为空".to_string()));
    }
    Ok(())
}

/// 从 tar 冗长列表行解析成员名与可选的符号链接目标。
pub(super) fn parse_tar_listing_line(line: &str) -> (&str, Option<&str>) {
    // GNU tar 冗长列表：`权限 owner/group 大小 日期 时间 name [-> target]`。
    // 字段间空白数量不定（size 前有大量填充空格），因此按「空白分隔字段计数」
    // 跳过前 5 个不含空格的字段，剩余部分从第 6 个字段起是成员名（可含空格）。
    let rest = line.trim_start();
    let mut field_count = 0usize;
    let mut in_field = false;
    let mut name_start = rest.len();
    for (index, ch) in rest.char_indices() {
        if ch.is_whitespace() {
            if in_field {
                field_count += 1;
                in_field = false;
                if field_count == 5 {
                    name_start = index;
                    break;
                }
            }
        } else {
            in_field = true;
        }
    }
    let name_with_target = rest[name_start..].trim_start();
    match name_with_target.split_once(" -> ") {
        Some((name, target)) => (name.trim(), Some(target.trim())),
        None => (name_with_target.trim(), None),
    }
}

/// 安全成员路径：相对路径、无 `..`/`.` 组件、无 NUL、不以 `/` 开头。
pub(super) fn is_safe_member_path(path: &str) -> bool {
    // 目录成员名以 `/` 结尾，先剥掉再做逐组件校验。
    let path = path.trim_end_matches('/');
    if path.is_empty() || path.starts_with('/') || path.contains('\0') {
        return false;
    }
    path.split('/')
        .all(|component| !component.is_empty() && component != ".." && component != ".")
}

/// 解压完成后兜底校验：work_dir 下每个真实路径（含通过符号链接到达的）必须
/// 位于输出目录内；发现越界项则删除并报错，阻断符号链接绕过。
pub(super) fn ensure_extracted_inside(output_dir: &Path) -> Result<()> {
    let output_dir = output_dir
        .canonicalize()
        .map_err(|e| AppError::Internal(format!("解析解压目录失败: {}", e)))?;
    let mut stack = vec![output_dir.clone()];
    let mut offenders = Vec::new();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let canonical = match path.canonicalize() {
                Ok(canonical) => canonical,
                Err(_) => {
                    offenders.push(path);
                    continue;
                }
            };
            if !canonical.starts_with(&output_dir) {
                offenders.push(path);
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            }
        }
    }
    if !offenders.is_empty() {
        let joined = offenders
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = std::fs::remove_dir_all(&output_dir);
        return Err(AppError::Validation(format!(
            "升级包解压结果越界，已回滚: {}",
            joined
        )));
    }
    Ok(())
}
