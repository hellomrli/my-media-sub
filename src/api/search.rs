use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::{timeout_at, Instant};

use super::response::ApiResponse as Response;
use crate::clients::{pansou::SearchResult, quark::QuarkShareInfo, PanSouClient, QuarkShareProbe};
use crate::error::{AppError, Result};
use crate::services::source_quality::{score_source, SourceQualityFile, SourceQualityInput};
use crate::store::SettingsStore;

/// 搜索请求的服务端总预算。
///
/// 低于 Nginx 默认的 60 秒读取超时，留出序列化与传输余量；
/// 推荐部署使用 90 秒反代超时，前端总超时为 120 秒。
const SEARCH_BUDGET: Duration = Duration::from_secs(50);

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

#[derive(Debug, Serialize)]
struct SearchResponse {
    // Keep data as an array for existing API clients.
    #[serde(flatten)]
    response: Response<Vec<SearchResult>>,
    partial: bool,
    timed_out: bool,
    probed_count: usize,
    total_count: usize,
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
    let probe_client = Arc::new(QuarkShareProbe::new(settings.quark_cookie.clone()));
    let max_files = if req.probe_files { req.max_files } else { 1 };
    execute_search(
        &req,
        deadline,
        pansou_client.search(&req.keyword, &settings.cloud_types, req.limit),
        move |result| {
            let probe_client = Arc::clone(&probe_client);
            async move {
                probe_client
                    .probe(&result.url, &result.password, max_files)
                    .await
            }
        },
    )
    .await
}

async fn execute_search<S, F, Fut>(
    req: &SearchRequest,
    deadline: Instant,
    search_results: S,
    probe: F,
) -> Result<Json<SearchResponse>>
where
    S: Future<Output = Result<Vec<SearchResult>>>,
    F: Fn(SearchResult) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = QuarkShareInfo> + Send + 'static,
{
    let mut results = match timeout_at(deadline, search_results).await {
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

    let total_count = results.len();
    let mut probed_count = 0;
    let mut partial = false;
    let mut timed_out = false;

    // 如果需要检测链接有效性或嗅探文件列表
    if req.check_links || req.probe_files {
        let probed = probe_all(results, MAX_CONCURRENT_PROBES, deadline, probe).await;
        probed_count = probed
            .results
            .iter()
            .filter(|(_, info)| info.is_some())
            .count();
        partial = probed_count < total_count;
        timed_out = probed.timed_out;
        results = probed
            .results
            .into_iter()
            .filter_map(|(mut result, probe_result)| {
                let Some(probe_result) = probe_result else {
                    // Unknown is not invalid. Preserve unprobed items in "keep all" mode.
                    return (!req.filter_bad).then_some(result);
                };
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

    let response = if partial {
        Response::with_message(
            results,
            format!(
                "{}，已完成 {}/{} 条资源探测；未完成项的有效性未知，请稍后重试或关闭探测后搜索",
                if timed_out {
                    "资源探测超时"
                } else {
                    "部分资源探测未完成"
                },
                probed_count,
                total_count
            ),
        )
    } else {
        Response::ok(results)
    };
    Ok(Json(SearchResponse {
        response,
        partial,
        timed_out,
        probed_count,
        total_count,
    }))
}

#[derive(Debug, PartialEq)]
struct ProbeBatch<T, U> {
    results: Vec<(T, Option<U>)>,
    timed_out: bool,
}

/// 在总预算内并发探测搜索结果，保持输入顺序。
///
/// 原始条目留在调用侧；取消探测不会丢失它们，也不会把未知状态标成无效。
async fn probe_all<T, U, F, Fut>(
    results: Vec<T>,
    max_concurrency: usize,
    deadline: Instant,
    probe: F,
) -> ProbeBatch<T, U>
where
    T: Clone + Send + 'static,
    U: Send + 'static,
    F: Fn(T) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = U> + Send + 'static,
{
    let total = results.len();
    if total == 0 || Instant::now() >= deadline {
        return ProbeBatch {
            results: results.into_iter().map(|result| (result, None)).collect(),
            timed_out: total > 0,
        };
    }

    let semaphore = Arc::new(Semaphore::new(max_concurrency.max(1)));
    let mut set = JoinSet::new();
    for (index, result) in results.iter().cloned().enumerate() {
        let semaphore = Arc::clone(&semaphore);
        let probe = probe.clone();
        set.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .expect("搜索探测并发信号量不会关闭");
            (index, probe(result).await)
        });
    }

    let mut completed: Vec<Option<U>> = (0..total).map(|_| None).collect();
    let mut done = 0usize;
    let mut timed_out = false;

    loop {
        match timeout_at(deadline, set.join_next()).await {
            Ok(Some(Ok((index, info)))) => {
                completed[index] = Some(info);
                done += 1;
                if done == total {
                    break;
                }
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
                timed_out = true;
                break;
            }
        }
    }

    ProbeBatch {
        results: results.into_iter().zip(completed).collect(),
        timed_out,
    }
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
                value * 10
            },
        )
        .await;

        assert_eq!(
            probed.results,
            vec![
                (1, Some(10)),
                (2, Some(20)),
                (3, Some(30)),
                (4, Some(40)),
                (5, Some(50))
            ]
        );
        assert!(!probed.timed_out);
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
                value
            },
        )
        .await;

        assert_eq!(
            probed.results,
            vec![(1, Some(1)), (2, None), (3, None), (4, None)]
        );
        assert!(probed.timed_out);
    }

    #[tokio::test]
    async fn expired_budget_preserves_unprobed_items() {
        let probed = probe_all(vec![1, 2], 2, Instant::now(), |_| async {
            panic!("expired probes must not start");
        })
        .await;
        assert_eq!(probed.results, vec![(1, None), (2, None)]);
        assert!(probed.timed_out);
        let empty = probe_all(Vec::<u32>::new(), 2, Instant::now(), |n| async move { n }).await;
        assert!(!empty.timed_out);
    }

    #[tokio::test]
    async fn probe_panics_preserve_unknown_items_without_claiming_timeout() {
        let probed = probe_all(
            vec![1, 2],
            2,
            Instant::now() + Duration::from_secs(5),
            |n| async move {
                assert_ne!(n, 1, "injected probe panic");
                n
            },
        )
        .await;
        assert_eq!(probed.results, vec![(1, None), (2, Some(2))]);
        assert!(!probed.timed_out);
    }

    #[tokio::test]
    async fn probes_respect_concurrency_limit() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let observed_peak = peak.clone();
        let probed = probe_all(
            (0..20).collect(),
            3,
            Instant::now() + Duration::from_secs(5),
            move |n| {
                let active = active.clone();
                let peak = peak.clone();
                async move {
                    let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(count, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    active.fetch_sub(1, Ordering::SeqCst);
                    n
                }
            },
        )
        .await;
        assert_eq!(probed.results.len(), 20);
        assert_eq!(observed_peak.load(Ordering::SeqCst), 3);
        assert!(!probed.timed_out);
    }

    fn result(id: &str) -> SearchResult {
        serde_json::from_value(serde_json::json!({
            "unique_id": id, "note": "Test", "url": "https://pan.quark.cn/s/test",
            "password": "", "source": "test", "datetime": "", "images": [], "cloud_type": "quark"
        }))
        .unwrap()
    }

    fn probe_info(ok: bool) -> QuarkShareInfo {
        QuarkShareInfo {
            ok,
            state: if ok { "ok" } else { "invalid" }.into(),
            message: String::new(),
            files: vec![],
            file_count: 0,
            episode_count: 0,
        }
    }

    // Exercise the same request parsing, search/probe pipeline and HTTP envelope
    // as production, with local futures instead of live PanSou/Quark services.
    async fn request(body: serde_json::Value, complete_one: bool) -> serde_json::Value {
        use axum::{
            body::{to_bytes, Body},
            http::{Request, StatusCode},
        };
        use tower::ServiceExt;
        let app = Router::new().route(
            "/api/search",
            post(move |Json(req): Json<SearchRequest>| async move {
                execute_search(
                    &req,
                    Instant::now() + Duration::from_millis(50),
                    async { Ok(vec![result("first"), result("second")]) },
                    move |result| async move {
                        if complete_one && result.unique_id == "first" {
                            probe_info(true)
                        } else {
                            std::future::pending().await
                        }
                    },
                )
                .await
            }),
        );
        let response = app
            .oneshot(
                Request::post("/api/search")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn search_http_reports_partial_and_preserves_unknown_results() {
        for complete_one in [false, true] {
            for filter_bad in [false, true] {
                let response = request(
                    serde_json::json!({
                        "keyword": "test", "probe_files": true, "filter_bad": filter_bad
                    }),
                    complete_one,
                )
                .await;
                assert_eq!(response["ok"], true);
                assert_eq!(response["partial"], true);
                assert_eq!(response["timed_out"], true);
                assert_eq!(response["total_count"], 2);
                assert_eq!(response["probed_count"], usize::from(complete_one));
                assert!(response["message"].as_str().unwrap().contains("超时"));
                let data = response["data"].as_array().unwrap();
                assert_eq!(
                    data.len(),
                    if filter_bad {
                        usize::from(complete_one)
                    } else {
                        2
                    }
                );
                if !filter_bad {
                    assert_eq!(data[1]["unique_id"], "second");
                    assert!(data[1].get("probe_info").is_none());
                    assert!(data[1].get("is_valid").is_none());
                }
            }
        }
    }

    #[tokio::test]
    async fn search_without_probing_is_complete() {
        let response = request(serde_json::json!({"keyword": "test"}), false).await;
        assert_eq!(response["partial"], false);
        assert_eq!(response["timed_out"], false);
        assert_eq!(response["probed_count"], 0);
        assert_eq!(response["total_count"], 2);
        assert_eq!(response["data"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn completed_link_checks_still_filter_invalid_results() {
        let req = SearchRequest {
            keyword: "test".into(),
            limit: 10,
            check_links: true,
            probe_files: false,
            max_files: 1,
            filter_bad: true,
        };
        let Json(response) = execute_search(
            &req,
            Instant::now() + Duration::from_secs(5),
            async { Ok(vec![result("first"), result("second")]) },
            |result| async move { probe_info(result.unique_id == "first") },
        )
        .await
        .unwrap();
        assert!(!response.partial);
        assert!(!response.timed_out);
        assert_eq!(response.probed_count, 2);
        let results = response.response.data.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].is_valid, Some(true));
    }
}
