use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

use crate::clients::quark::{QuarkFile, QuarkShareProbe};
use crate::clients::quark_save::{NormalizedItem, QuarkSaveClient};
use crate::error::{AppError, Result};
use crate::models::subscription::Subscription;
use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};

/// 递归收集目录下所有视频文件（独立函数，使用 Box 解决递归问题）
fn collect_video_files_recursive<'a>(
    save_client: &'a QuarkSaveClient,
    parent_fid: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<NormalizedItem>>> + Send + 'a>> {
    Box::pin(async move {
        use crate::services::is_video_name;
        let mut video_files = Vec::new();

        let items = save_client.list_dir(parent_fid).await?;

        for item in items {
            if item.is_dir {
                // 递归进入子目录
                match collect_video_files_recursive(save_client, &item.fid).await {
                    Ok(mut sub_videos) => video_files.append(&mut sub_videos),
                    Err(e) => warn!("读取子目录 {} 失败: {}", item.file_name, e),
                }
            } else if is_video_name(&item.file_name) {
                // 是视频文件，加入列表
                video_files.push(item);
            }
        }

        Ok(video_files)
    })
}

/// 订阅自动转存服务
pub struct SubscriptionTransferService {
    subscription_store: Arc<SubscriptionStore>,
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
}

impl SubscriptionTransferService {
    pub fn new(
        subscription_store: Arc<SubscriptionStore>,
        settings_store: Arc<SettingsStore>,
        notification_store: Arc<NotificationStore>,
    ) -> Self {
        Self {
            subscription_store,
            settings_store,
            notification_store,
        }
    }

    /// 自动转存订阅的新文件
    /// 在 check_subscription 发现新文件后调用
    pub async fn auto_transfer_new_files(
        &self,
        subscription_id: &str,
        new_file_names: &[String],
    ) -> Result<TransferResult> {
        let sub = self
            .subscription_store
            .get(subscription_id)
            .await
            .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;

        // 检查是否启用自动转存
        if sub.notify_only {
            return Ok(TransferResult {
                subscription_id: sub.id.clone(),
                transferred_count: 0,
                skipped: true,
                reason: "订阅设置为仅通知模式".to_string(),
            });
        }

        let settings = self.settings_store.get().await;

        if !settings.quark_save_enabled {
            return Ok(TransferResult {
                subscription_id: sub.id.clone(),
                transferred_count: 0,
                skipped: true,
                reason: "全局自动转存未启用".to_string(),
            });
        }

        let cookie = settings.quark_cookie.clone();
        if cookie.is_empty() {
            return Ok(TransferResult {
                subscription_id: sub.id.clone(),
                transferred_count: 0,
                skipped: true,
                reason: "未配置夸克 Cookie".to_string(),
            });
        }

        if new_file_names.is_empty() {
            return Ok(TransferResult {
                subscription_id: sub.id.clone(),
                transferred_count: 0,
                skipped: true,
                reason: "无新文件需要转存".to_string(),
            });
        }

        info!(
            "开始自动转存订阅 {} 的 {} 个新文件",
            sub.title,
            new_file_names.len()
        );

        // 1. 探测分享链接获取文件信息
        let probe = QuarkShareProbe::new(cookie.clone());
        let share_info = probe.probe(&sub.url, &sub.password, 200).await;

        if !share_info.ok {
            warn!("探测分享链接失败: {}", share_info.message);
            return Err(AppError::Http(format!(
                "探测分享链接失败: {}",
                share_info.message
            )));
        }

        // 2. 筛选出新文件
        let files_to_transfer: Vec<&QuarkFile> = share_info
            .files
            .iter()
            .filter(|f| new_file_names.contains(&f.name))
            .collect();

        if files_to_transfer.is_empty() {
            return Ok(TransferResult {
                subscription_id: sub.id.clone(),
                transferred_count: 0,
                skipped: true,
                reason: "未找到匹配的文件".to_string(),
            });
        }

        // 3. 确定目标目录
        let target_dir = self.determine_target_directory(&sub, &settings);
        let save_client = QuarkSaveClient::new(cookie.clone());

        let target_fid = if target_dir.is_empty() || target_dir == "/" {
            "0".to_string()
        } else {
            match save_client.ensure_dir_path(&target_dir).await {
                Ok(fid) => fid,
                Err(e) => {
                    warn!("创建/查找目标目录失败: {}, 使用根目录", e);
                    "0".to_string()
                }
            }
        };

        // 4. 提取 pwd_id
        let pwd_id = match QuarkShareProbe::extract_pwd_id(&sub.url) {
            Some(id) => id,
            None => {
                return Err(AppError::Validation("无法提取分享链接 ID".to_string()));
            }
        };

        // 5. 重新获取最新的 stoken 和 share_fid_token
        let (stoken, err) = probe.get_share_token(&pwd_id, &sub.password).await?;
        if let Some(err_msg) = err {
            return Err(AppError::Http(format!("获取分享 token 失败: {}", err_msg)));
        }

        let stoken = stoken.ok_or_else(|| AppError::Http("未能获取分享 token".to_string()))?;

        // 6. 重新列出文件获取最新 token
        let (fresh_files, err) = probe.list_share_files(&pwd_id, &stoken, "0").await?;
        if let Some(err_msg) = err {
            return Err(AppError::Http(format!("获取文件列表失败: {}", err_msg)));
        }

        // 7. 收集 fid 和 share_fid_token
        let mut fid_list = Vec::new();
        let mut fid_token_list = Vec::new();

        for item in &fresh_files {
            let fid = item
                .get("fid")
                .or_else(|| item.get("file_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let name = item
                .get("file_name")
                .or_else(|| item.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let share_fid_token = item
                .get("share_fid_token")
                .or_else(|| item.get("file_token"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // 只转存新文件
            if !fid.is_empty()
                && !share_fid_token.is_empty()
                && new_file_names.contains(&name.to_string())
            {
                fid_list.push(fid.to_string());
                fid_token_list.push(share_fid_token.to_string());
            }
        }

        if fid_list.is_empty() {
            return Err(AppError::Validation(
                "没有可转存的文件（缺少 fid 或 token）".to_string(),
            ));
        }

        // 8. 执行转存
        info!("转存 {} 个文件到 {}", fid_list.len(), target_dir);
        save_client
            .save_share_files(&pwd_id, &stoken, &fid_list, &fid_token_list, &target_fid)
            .await?;

        // 9. 如果设置了重命名模板，执行重命名
        if !sub.rules.rename_template.is_empty() {
            info!("开始重命名文件，模板: {}", sub.rules.rename_template);
            self.rename_transferred_files(&save_client, &target_fid, &sub)
                .await?;
        }

        // 10. 更新订阅的 transferred_files
        self.mark_files_as_transferred(&sub.id, new_file_names)
            .await?;

        // 11. 发送转存成功通知
        self.send_transfer_notification(&sub, new_file_names, &target_dir)
            .await;

        info!("成功转存 {} 个文件", fid_list.len());

        Ok(TransferResult {
            subscription_id: sub.id.clone(),
            transferred_count: fid_list.len(),
            skipped: false,
            reason: format!("已转存到 {}", target_dir),
        })
    }

    /// 重命名转存后的文件
    async fn rename_transferred_files(
        &self,
        save_client: &QuarkSaveClient,
        target_fid: &str,
        sub: &Subscription,
    ) -> Result<usize> {
        use crate::services::detect_episode;

        info!("开始重命名文件，目标目录 fid: {}", target_fid);

        // 夸克转存接口可能先返回成功，再异步落盘；等待目标目录出现视频文件。
        let mut video_files = Vec::new();
        for attempt in 1..=10 {
            video_files = collect_video_files_recursive(save_client, target_fid).await?;
            if !video_files.is_empty() {
                break;
            }
            info!("目标目录暂未看到视频文件，等待后重试 ({}/10)", attempt);
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        info!("找到 {} 个视频文件", video_files.len());

        let mut renamed_count = 0;

        // 按订阅模板重命名目标目录下能识别集数的视频文件。
        for video_file in &video_files {
            // 提取集数
            let episode_info = detect_episode(&video_file.file_name);
            if episode_info.episode.is_none() {
                info!("无法从 {} 提取集数，跳过重命名", video_file.file_name);
                continue;
            }

            let episode_num = episode_info.episode.unwrap();

            // 生成新文件名
            let new_name = if sub.rules.rename_template.contains("{}") {
                // 模板格式: "动画名称.S01E{}"
                let ext = std::path::Path::new(&video_file.file_name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("mkv");
                format!(
                    "{}.{}",
                    sub.rules
                        .rename_template
                        .replace("{}", &format!("{:02}", episode_num)),
                    ext
                )
            } else {
                warn!("重命名模板格式不正确: {}", sub.rules.rename_template);
                continue;
            };

            // 如果新旧文件名相同，跳过
            if new_name == video_file.file_name {
                info!("文件名已经匹配模板，跳过: {}", video_file.file_name);
                continue;
            }

            // 执行重命名
            info!("重命名: {} -> {}", video_file.file_name, new_name);
            let parent_fid = if video_file.parent_fid.trim().is_empty() {
                None
            } else {
                Some(video_file.parent_fid.as_str())
            };
            match save_client
                .rename_item(&video_file.fid, &new_name, parent_fid)
                .await
            {
                Ok(_) => {
                    renamed_count += 1;
                    info!("重命名成功: {}", new_name);
                }
                Err(e) => warn!("重命名失败 {}: {}", video_file.file_name, e),
            }
        }

        Ok(renamed_count)
    }

    /// 按订阅规则重命名目标目录中的现有视频文件。
    pub async fn rename_existing_files(&self, subscription_id: &str) -> Result<usize> {
        let sub = self
            .subscription_store
            .get(subscription_id)
            .await
            .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;

        if sub.rules.rename_template.trim().is_empty() {
            return Err(AppError::Validation("订阅未配置重命名模板".to_string()));
        }

        let settings = self.settings_store.get().await;
        if settings.quark_cookie.trim().is_empty() {
            return Err(AppError::Validation("未配置夸克 Cookie".to_string()));
        }

        let save_client = QuarkSaveClient::new(settings.quark_cookie.clone());
        let target_dir = self.determine_target_directory(&sub, &settings);
        let target_fid = save_client.ensure_dir_path(&target_dir).await?;

        info!(
            "开始修复订阅 {} 目标目录 {} 的文件命名",
            sub.title, target_dir
        );
        self.rename_transferred_files(&save_client, &target_fid, &sub)
            .await
    }

    /// 确定目标目录
    fn determine_target_directory(
        &self,
        sub: &Subscription,
        settings: &crate::models::Settings,
    ) -> String {
        // 如果订阅设置了 target_dir，优先使用
        if !sub.rules.target_dir.is_empty() {
            return sub.rules.target_dir.clone();
        }

        // 否则根据媒体类型选择分类目录
        let base = if settings.quark_save_root.is_empty() {
            String::new()
        } else {
            settings.quark_save_root.clone()
        };

        let category_dir = match sub.media_type.as_str() {
            "movie" => &settings.quark_save_movie_dir,
            "series" => &settings.quark_save_series_dir,
            "anime" => &settings.quark_save_anime_dir,
            _ => {
                // 检查自定义分类
                for cat in &settings.custom_categories {
                    if sub.media_type == format!("custom_{}", cat.id) {
                        return cat.dir.clone();
                    }
                }
                return base;
            }
        };

        if category_dir.is_empty() {
            base
        } else {
            category_dir.clone()
        }
    }

    /// 标记文件为已转存
    async fn mark_files_as_transferred(
        &self,
        subscription_id: &str,
        file_names: &[String],
    ) -> Result<()> {
        self.subscription_store
            .update(subscription_id, |sub| {
                for name in file_names {
                    if !sub.transferred_files.contains(name) {
                        sub.transferred_files.push(name.clone());
                    }
                }
                sub.updated_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
            })
            .await?;

        Ok(())
    }

    /// 发送转存通知
    async fn send_transfer_notification(
        &self,
        sub: &Subscription,
        file_names: &[String],
        target_dir: &str,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let message = format!(
            "已转存 {} 个文件到 {}",
            file_names.len(),
            if target_dir.is_empty() {
                "根目录"
            } else {
                target_dir
            }
        );

        let notification = crate::models::Notification {
            id: uuid::Uuid::new_v4().to_string(),
            level: "success".to_string(),
            event: "subscription_transferred".to_string(),
            title: format!("订阅自动转存: {}", sub.title),
            message,
            meta: std::collections::HashMap::new(),
            read: false,
            created_at: now,
        };

        let _ = self.notification_store.add(notification).await;
    }
}

/// 转存结果
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TransferResult {
    pub subscription_id: String,
    pub transferred_count: usize,
    pub skipped: bool,
    pub reason: String,
}
