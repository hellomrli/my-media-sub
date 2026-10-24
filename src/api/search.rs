use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use serde::Deserialize;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::{timeout_at, Instant};

use super::response::ApiResponse as Response;
use crate::clients::{PanSouClient, QuarkShareProbe};
use crate::error::{AppError, Result};
use crate::services::source_quality::{score_source, SourceQualityFile, SourceQualityInput};
use crate::store::SettingsStore;

/// 搜索请求的服务端总预算。
///
/// 前端对 `/api/search` 使用 120 秒总超时；这里留出余量，
/// 到点后返回已经探测完成的结果，避免浏览器先超时、用户只看到红字。
const SEARCH_BUDGET: Duration = Duration::from_secs(60);

/// 单次搜索同时探测的分享链接数。
///
/// 50 条结果串行探测约需 2 分钟；8 路并发通常可压到 15 秒左右，
/// 同时避免对夸克接口产生过大瞬时压力。
const MAX_CONCURRENT_PROBES: usize = 8;

/// 搜索路由状态
pub struct SearchState {
    pub settings_store: Arc<SettingsStore>,
}

/// 搜索请求
#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub keyword: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// 是否检测链接有效性
    #[serde(default)]
    pub check_links: bool,
    /// 是否嗅探文件列表
    #[serde(default)]
    pub probe_files: bool,
    /// 嗅探时获取的最大文件数
    #[serde(default = "default_max_files")]
    pub max_files: usize,
    /// 是否过滤失效链接（需要 check_links 为 true）
    #[serde(default)]
    pub filter_bad: bool,
}

fn default_limit() -> usize {
    10
}

fn default_max_files() -> usize {
    50
}

/// 搜索资源
async fn search(
    State(state): State<Arc<SearchState>>,
    Json(req): Json<SearchRequest>,
) -> Result<impl IntoResponse> {
    let deadline = Instant::now() + SEARCH_BUDGET;
    let settings = state.settings_store.get().await;
    let pansou_api_url = settings.pansou_api_url.trim().to_string();
    let pansou_api_url = if pansou_api_url.is_empty() {
        None
    } else {
        Some(pansou_api_url)
    };
    let pansou_client = PanSouClient::new(pansou_api_url);
    let mut results = match timeout_at(
        deadline,
        pansou_client.search(&req.keyword, &settings.cloud_types, req.limit),
    )
    .await
    {
        Ok(result) => result?,
        Err(_) => {
            tracing::warn!(
                budget_secs = SEARCH_BUDGET.as_secs(),
                "搜索总时限已到，PanSou 阶段未完成"
            );
            return Err(AppError::Http(format!(
                "搜索超时（{} 秒），请稍后重试",
                SEARCH_BUDGET.as_secs()
            )));
        }
    };

    // 如果需要检测链接有效性或嗅探文件列表
    if req.check_links || req.probe_files {
        let max_files = if req.probe_files { req.max_files } else { 1 };
        let probe_client = Arc::new(QuarkShareProbe::new(settings.quark_cookie.clone()));
        let probed = probe_all(results, MAX_CONCURRENT_PROBES, deadline, move |result| {
            let probe_client = Arc::clone(&probe_client);
            async move {
                let probe_result = probe_client
                    .probe(&result.url, &result.password, max_files)
                    .await;
                (result, probe_result)
            }
        })
        .await;

        results = probed
            .into_iter()
            .filter_map(|(mut result, probe_result)| {
                // 如果需要过滤失效链接，跳过探测失败的项。
                if req.filter_bad && !probe_result.ok {
                    return None;
                }

                if req.probe_files {
                    result.probe_info = Some(probe_result);
                } else if req.check_links {
                    // 只检测有效性，不获取文件列表。
                    result.is_valid = Some(probe_result.ok);
                }

                Some(result)
            })
            .collect();
    }

    let now_ms = chrono::Utc::now().timestamp_millis();
    for result in &mut results {
        let probe = result.probe_info.as_ref();
        result.quality = score_source(
            &SourceQualityInput {
                title: result.note.clone(),
                datetime: result.datetime.clone(),
                validity: result.is_valid,
                probe_ok: probe.map(|probe| probe.ok),
                probe_file_count: probe.map(|probe| probe.file_count).unwrap_or_default(),
                probe_episode_count: probe.map(|probe| probe.episode_count).unwrap_or_default(),
                files: probe
                    .map(|probe| {
                        probe
                            .files
                            .iter()
                            .map(|file| SourceQualityFile {
                                name: file.name.clone(),
                                is_dir: file.is_dir,
                                size: file.size,
                                updated_at: file.updated_at.clone(),
                                category: file.category.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            now_ms,
        );
    }

    Ok(Json(Response::ok(results)))
}

/// 在总预算内并发探测搜索结果，保持输入顺序。
///
/// 达到 `deadline` 时中止未完成任务，只返回已经完成的部分；
/// 因此慢请求不会把整个搜索拖到前端超时。
async fn probe_all<T, U, F, Fut>(
    results: Vec<T>,
    max_concurrency: usize,
    deadline: Instant,
    probe: F,
) -> Vec<(T, U)>
where
    T: Send + 'static,
    U: Send + 'static,
    F: Fn(T) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = (T, U)> + Send + 'static,
{
    let total = results.len();
    if total == 0 || Instant::now() >= deadline {
        return Vec::new();
    }

    let semaphore = Arc::new(Semaphore::new(max_concurrency.max(1)));
    let mut set = JoinSet::new();
    for (index, result) in results.into_iter().enumerate() {
        let semaphore = Arc::clone(&semaphore);
        let probe = probe.clone();
        set.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .expect("搜索探测并发信号量不会关闭");
            let (result, info) = probe(result).await;
            (index, result, info)
        });
    }

    let mut completed: Vec<Option<(T, U)>> = (0..total).map(|_| None).collect();
    let mut done = 0usize;

    loop {
        match timeout_at(deadline, set.join_next()).await {
            Ok(Some(Ok((index, result, info)))) => {
                completed[index] = Some((result, info));
                done += 1;
            }
            Ok(Some(Err(error))) => {
                tracing::warn!(error = %error, "搜索探测任务异常结束");
            }
            Ok(None) => break,
            Err(_) => {
                tracing::warn!(
                    completed = done,
                    total,
                    budget_secs = SEARCH_BUDGET.as_secs(),
                    "搜索探测达到总时限，返回部分结果"
                );
                set.abort_all();
                break;
            }
        }
    }

    completed.into_iter().flatten().collect()
}

/// 创建搜索路由
pub fn routes(settings_store: Arc<SettingsStore>) -> Router {
    let state = Arc::new(SearchState { settings_store });

    Router::new()
        .route("/api/search", post(search))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn probe_all_preserves_input_order() {
        let results = vec![1_u32, 2, 3, 4, 5];
        let probed = probe_all(
            results,
            2,
            Instant::now() + Duration::from_secs(5),
            |value| async move {
                tokio::time::sleep(Duration::from_millis(5)).await;
                (value, value * 10)
            },
        )
        .await;

        assert_eq!(probed, vec![(1, 10), (2, 20), (3, 30), (4, 40), (5, 50)]);
    }

    #[tokio::test]
    async fn probe_all_returns_partial_results_at_deadline() {
        let results = vec![1_u32, 2, 3, 4];
        let probed = probe_all(
            results,
            2,
            Instant::now() + Duration::from_millis(200),
            |value| async move {
                let delay = if value == 1 { 10 } else { 5_000 };
                tokio::time::sleep(Duration::from_millis(delay)).await;
                (value, value)
            },
        )
        .await;

        assert_eq!(probed, vec![(1, 1)]);
    }
}
