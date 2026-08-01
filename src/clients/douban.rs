//! 豆瓣电影 subject 解析：仅处理 movie.douban.com 电影链接，取片名供搜索。

use crate::clients::http_pool::ObservedRequestBuilder;
use crate::error::{AppError, Result};
use regex::Regex;
use reqwest::Client;
use std::sync::LazyLock;

// 桌面版 movie.douban.com 对数据中心 IP 常返回工作量证明验证页；
// 移动版 m.douban.com/subject/<id>/ 不需要过验证即可拿到完整页面。
const DOUBAN_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";

/// 只匹配豆瓣电影 subject 链接（movie.douban.com / m.douban.com）。
static SUBJECT_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)https?://(?:movie|m)\.douban\.com/(?:movie/)?subject/(\d+)"#)
        .expect("douban movie subject URL regex")
});

static TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<title[^>]*>(.*?)</title>").expect("douban title regex"));

static OG_TITLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<meta[^>]+property=["']og:title["'][^>]+content=["'](.*?)["']"#)
        .expect("douban og:title regex")
});

static YEAR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[（(](\d{4})[)）]").expect("douban year regex"));

static RELEASE_DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<meta[^>]+property=["']video:release_date["'][^>]+content=["'](\d{4})"#)
        .expect("douban release date regex")
});

static ORIGINAL_TITLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)class=["']sub-original-title["'][^>]*>(.*?)<"#)
        .expect("douban original title regex")
});

/// 从豆瓣电影 subject 页解析出的基本信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoubanSubject {
    pub id: String,
    pub title: String,
    pub year: Option<String>,
}

/// 从任意文本里找到第一个豆瓣电影 subject 链接，返回规范化后的桌面版 URL。
pub fn find_subject_url(text: &str) -> Option<String> {
    let id = SUBJECT_URL_RE.captures(text)?.get(1)?.as_str();
    Some(format!("https://movie.douban.com/subject/{id}/"))
}

/// 抓取豆瓣电影页面并解析片名、年份。
pub async fn fetch_subject(client: &Client, url: &str) -> Result<DoubanSubject> {
    let id = SUBJECT_URL_RE
        .captures(url)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_string())
        .ok_or_else(|| AppError::Validation("不是有效的豆瓣电影 subject 链接".to_string()))?;

    let page_url = format!("https://m.douban.com/subject/{id}/");
    let response = client
        .get(&page_url)
        .header(reqwest::header::USER_AGENT, DOUBAN_UA)
        .header(reqwest::header::ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9,en;q=0.8")
        .send_observed("douban")
        .await
        .map_err(|error| AppError::Http(format!("豆瓣页面请求失败: {error}")))?;
    if !response.status().is_success() {
        return Err(AppError::Http(format!(
            "豆瓣页面返回 HTTP {}",
            response.status()
        )));
    }
    let html = response
        .text()
        .await
        .map_err(|error| AppError::Http(format!("读取豆瓣页面失败: {error}")))?;

    let title = extract_title(&html)
        .ok_or_else(|| AppError::Http("豆瓣页面未找到片名（页面结构可能已变化）".to_string()))?;
    let year = extract_year(&html, &title);
    Ok(DoubanSubject { id, title, year })
}

fn extract_title(html: &str) -> Option<String> {
    if let Some(captures) = OG_TITLE_RE.captures(html) {
        let title = strip_douban_suffix(&decode_html_entities(captures.get(1)?.as_str().trim()));
        if !title.is_empty() {
            return Some(title);
        }
    }
    let raw = decode_html_entities(TITLE_RE.captures(html)?.get(1)?.as_str().trim());
    let title = strip_douban_suffix(raw.strip_suffix(" - 豆瓣").unwrap_or(&raw).trim());
    (!title.is_empty()).then(|| title.to_string())
}

fn extract_year(html: &str, title: &str) -> Option<String> {
    if let Some(captures) = YEAR_RE.captures(title) {
        return captures.get(1).map(|value| value.as_str().to_string());
    }
    if let Some(captures) = TITLE_RE.captures(html) {
        let raw_title = decode_html_entities(captures.get(1)?.as_str());
        if let Some(captures) = YEAR_RE.captures(&raw_title) {
            return captures.get(1).map(|value| value.as_str().to_string());
        }
    }
    if let Some(captures) = ORIGINAL_TITLE_RE.captures(html) {
        let original = decode_html_entities(captures.get(1)?.as_str());
        if let Some(captures) = YEAR_RE.captures(&original) {
            return captures.get(1).map(|value| value.as_str().to_string());
        }
    }
    if let Some(captures) = RELEASE_DATE_RE.captures(html) {
        return captures.get(1).map(|value| value.as_str().to_string());
    }
    None
}

/// 去掉豆瓣标题末尾的类型后缀，如「庆余年 第一季 - 电视剧」。
fn strip_douban_suffix(value: &str) -> String {
    let parts = value.split(" - ").collect::<Vec<_>>();
    if parts.len() > 1 {
        parts[..parts.len() - 1].join(" - ")
    } else {
        value.to_string()
    }
}

fn decode_html_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clients::http_pool;

    #[test]
    fn finds_only_douban_movie_subject_links() {
        assert_eq!(
            find_subject_url("https://movie.douban.com/subject/35445285/").as_deref(),
            Some("https://movie.douban.com/subject/35445285/")
        );
        assert_eq!(
            find_subject_url(
                "看看这个 https://m.douban.com/movie/subject/35445285/?from=wechat 怎么样"
            )
            .as_deref(),
            Some("https://movie.douban.com/subject/35445285/")
        );
        assert_eq!(
            find_subject_url("https://m.douban.com/subject/25853071/"),
            Some("https://movie.douban.com/subject/25853071/".to_string())
        );
        // 非电影域名（图书/音乐等通用 subject）不处理。
        assert_eq!(
            find_subject_url("https://www.douban.com/subject/26831934/"),
            None
        );
        assert_eq!(find_subject_url("https://example.com/subject/1/"), None);
        assert_eq!(find_subject_url("没有链接"), None);
    }

    #[test]
    fn parses_title_and_year_from_douban_html() {
        let html = r#"<html><head>
            <title>庆余年 第一季 (2019) (豆瓣)</title>
            <meta property="og:title" content="庆余年 第一季 - 电视剧">
        </head><body></body></html>"#;
        assert_eq!(extract_title(html).as_deref(), Some("庆余年 第一季"));
        assert_eq!(extract_year(html, "庆余年 第一季").as_deref(), Some("2019"));
    }

    #[test]
    fn falls_back_to_title_tag_and_decodes_entities() {
        let html = "<html><title>Fringe &amp; 危机边缘 - 电视剧 (2008) - 豆瓣</title></html>";
        assert_eq!(extract_title(html).as_deref(), Some("Fringe & 危机边缘"));
        assert_eq!(
            extract_year(html, "Fringe & 危机边缘 (2008)").as_deref(),
            Some("2008")
        );
    }

    #[test]
    fn mobile_page_year_comes_from_original_title_block() {
        let html = r#"<html><head>
            <meta property="og:title" content="庆余年 第一季 - 电视剧">
        </head><body>
            <div class="sub-original-title">庆余年 第一季（2019）</div>
        </body></html>"#;
        assert_eq!(extract_title(html).as_deref(), Some("庆余年 第一季"));
        assert_eq!(extract_year(html, "庆余年 第一季").as_deref(), Some("2019"));
    }

    #[tokio::test]
    async fn fetch_rejects_non_subject_url() {
        let client = http_pool::short_client();
        let error = fetch_subject(&client, "https://example.com/subject/1/")
            .await
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("不是有效的豆瓣电影 subject 链接"));
    }

    // 真实豆瓣页面测试（需要网络）
    #[tokio::test]
    #[ignore]
    async fn fetch_real_movie_subject() {
        let client = http_pool::short_client();
        let subject = fetch_subject(&client, "https://movie.douban.com/subject/36857924/")
            .await
            .unwrap();
        assert!(!subject.title.is_empty());
        println!("{:?}", subject);
    }
}
