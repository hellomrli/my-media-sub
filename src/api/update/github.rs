use super::*;

pub(super) async fn fetch_latest_release() -> Result<GithubRelease> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );
    let client = http_pool::default_client();
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "my-media-sub-update-check")
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json::<GithubRelease>().await?)
}

pub(super) async fn fetch_release_by_tag(tag: &str) -> Result<GithubRelease> {
    let tag = tag.trim().trim_start_matches('/').to_string();
    // 只接受发布标签的合法字符，防止 `?`/`#` 等改写 GitHub API 请求语义。
    let tag_is_valid = !tag.is_empty()
        && !tag.contains('/')
        && tag
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '+'));
    if !tag_is_valid {
        return Err(AppError::Validation("版本标签无效".to_string()));
    }

    let url = format!(
        "https://api.github.com/repos/{}/releases/tags/{}",
        GITHUB_REPO, tag
    );
    let client = http_pool::default_client();
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "my-media-sub-update-check")
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json::<GithubRelease>().await?)
}

pub(super) async fn fetch_releases() -> Result<Vec<GithubRelease>> {
    let url = format!(
        "https://api.github.com/repos/{}/releases?per_page=20",
        GITHUB_REPO
    );
    let client = http_pool::default_client();
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "my-media-sub-update-check")
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json::<Vec<GithubRelease>>().await?)
}

pub(super) fn release_to_response(
    release: GithubRelease,
    current_version: &str,
) -> UpdateReleaseResponse {
    let version = normalize_version(&release.tag_name);
    let is_current = version == current_version;
    let is_newer = is_newer_version(&version, current_version);
    UpdateReleaseResponse {
        tag: release.tag_name.clone(),
        version,
        name: release.name.unwrap_or_else(|| release.tag_name.clone()),
        release_url: release.html_url,
        published_at: release.published_at,
        asset: find_asset(&release.assets, "linux-x86_64.tar.gz").map(Into::into),
        is_current,
        is_newer,
    }
}
