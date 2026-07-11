use super::*;
use crate::clients::{QuarkSaveClient, QuarkShareProbe};
use crate::error::AppError;
use std::collections::{HashSet, VecDeque};

pub struct QuarkCloudDriveProvider {
    probe: QuarkShareProbe,
    drive: QuarkSaveClient,
}

impl QuarkCloudDriveProvider {
    pub fn new(cookie: impl Into<String>) -> Self {
        let cookie = cookie.into();
        Self {
            probe: QuarkShareProbe::new(cookie.clone()),
            drive: QuarkSaveClient::new(cookie),
        }
    }

    fn map_probe(info: crate::clients::quark::QuarkShareInfo) -> ProviderProbeResult {
        ProviderProbeResult {
            ok: info.ok,
            state: info.state,
            message: info.message,
            files: info.files.into_iter().map(map_share_file).collect(),
        }
    }
}

fn map_share_file(file: crate::clients::quark::QuarkFile) -> ProviderFile {
    ProviderFile {
        id: file.fid,
        name: file.name,
        is_dir: file.is_dir,
        size: file.size,
        parent_path: file.parent_path,
        updated_at: file.updated_at,
    }
}

fn map_drive_item(item: crate::clients::NormalizedItem) -> DriveItem {
    DriveItem {
        id: item.fid,
        parent_id: item.parent_fid,
        name: item.file_name,
        is_dir: item.is_dir,
        size: item.size,
        updated_at: item.updated_at,
    }
}

fn raw_string(
    item: &std::collections::HashMap<String, serde_json::Value>,
    keys: &[&str],
) -> String {
    keys.iter()
        .find_map(|key| item.get(*key).and_then(serde_json::Value::as_str))
        .unwrap_or("")
        .to_string()
}

fn raw_is_dir(item: &std::collections::HashMap<String, serde_json::Value>) -> bool {
    item.get("dir")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
        || item.get("file").and_then(serde_json::Value::as_bool) == Some(false)
        || (item.get("file_type").and_then(serde_json::Value::as_i64) == Some(0)
            && !item.contains_key("format_type")
            && item
                .get("size")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0)
                == 0)
}

impl CloudDriveProvider for QuarkCloudDriveProvider {
    fn cloud_type(&self) -> &'static str {
        "quark"
    }

    fn probe<'a>(
        &'a self,
        url: &'a str,
        passcode: &'a str,
        max_files: usize,
    ) -> ProviderFuture<'a, ProviderProbeResult> {
        Box::pin(async move {
            Ok(Self::map_probe(
                self.probe.probe(url, passcode, max_files).await,
            ))
        })
    }

    fn list<'a>(&'a self, parent_id: &'a str) -> ProviderFuture<'a, Vec<DriveItem>> {
        Box::pin(async move {
            Ok(self
                .drive
                .list_dir(parent_id)
                .await?
                .into_iter()
                .map(map_drive_item)
                .collect())
        })
    }

    fn find<'a>(
        &'a self,
        parent_id: &'a str,
        name: &'a str,
    ) -> ProviderFuture<'a, Option<DriveItem>> {
        Box::pin(async move {
            Ok(self
                .list(parent_id)
                .await?
                .into_iter()
                .find(|item| item.name == name))
        })
    }

    fn ensure<'a>(&'a self, path: &'a str) -> ProviderFuture<'a, String> {
        Box::pin(async move { self.drive.ensure_dir_path(path).await })
    }

    fn transfer<'a>(&'a self, request: TransferRequest) -> ProviderFuture<'a, TransferOutcome> {
        Box::pin(async move {
            let pwd_id = QuarkShareProbe::extract_pwd_id(&request.share_url)
                .ok_or_else(|| AppError::Validation("无法提取夸克分享链接 ID".to_string()))?;
            let (stoken, error) = self
                .probe
                .get_share_token(&pwd_id, &request.passcode)
                .await?;
            if let Some(error) = error {
                return Err(AppError::Http(format!("获取分享 token 失败: {error}")));
            }
            let stoken = stoken.ok_or_else(|| AppError::Http("未能获取分享 token".to_string()))?;
            // Refresh file tokens with the same stoken that will be used for saving.
            let selected: HashSet<&str> = request.file_ids.iter().map(String::as_str).collect();
            let mut files = Vec::new();
            let mut queue = VecDeque::from([("0".to_string(), String::new())]);
            let mut visited = HashSet::new();
            while let Some((parent_id, parent_path)) = queue.pop_front() {
                if !visited.insert(parent_id.clone()) {
                    continue;
                }
                let (items, error) = self
                    .probe
                    .list_share_files(&pwd_id, &stoken, &parent_id)
                    .await?;
                if let Some(error) = error {
                    return Err(AppError::Http(format!("重新获取分享文件列表失败: {error}")));
                }
                for item in items {
                    let fid = raw_string(&item, &["fid", "file_id"]);
                    let name = raw_string(&item, &["file_name", "name"]);
                    let is_dir = raw_is_dir(&item);
                    let include = if selected.is_empty() {
                        parent_id == "0"
                    } else {
                        !is_dir && selected.contains(fid.as_str())
                    };
                    if include {
                        files.push(crate::clients::quark::QuarkFile {
                            name: name.clone(),
                            fid: fid.clone(),
                            share_fid_token: raw_string(&item, &["share_fid_token", "file_token"]),
                            is_dir,
                            size: item
                                .get("size")
                                .and_then(serde_json::Value::as_i64)
                                .unwrap_or(0),
                            parent_path: parent_path.clone(),
                            updated_at: None,
                            category: None,
                            format_type: None,
                        });
                    }
                    if !selected.is_empty() && is_dir && !fid.is_empty() {
                        let child_path = if parent_path.is_empty() {
                            name
                        } else {
                            format!("{parent_path}/{name}")
                        };
                        queue.push_back((fid, child_path));
                    }
                }
            }
            let fid_list: Vec<_> = files.iter().map(|file| file.fid.clone()).collect();
            let token_list: Vec<_> = files
                .iter()
                .map(|file| file.share_fid_token.clone())
                .collect();
            if fid_list.is_empty() || token_list.iter().any(|token| token.is_empty()) {
                return Err(AppError::Validation(
                    "没有可转存的文件（缺少 fid 或 token）".to_string(),
                ));
            }
            self.drive
                .save_share_files(&pwd_id, &stoken, &fid_list, &token_list, &request.target_id)
                .await?;
            Ok(TransferOutcome {
                transferred_files: files.into_iter().map(map_share_file).collect(),
            })
        })
    }

    fn rename<'a>(
        &'a self,
        id: &'a str,
        name: &'a str,
        parent_id: Option<&'a str>,
    ) -> ProviderFuture<'a, ()> {
        Box::pin(async move { self.drive.rename_item(id, name, parent_id).await })
    }

    fn delete<'a>(&'a self, ids: &'a [String]) -> ProviderFuture<'a, ()> {
        Box::pin(async move { self.drive.delete_items(ids).await })
    }

    fn download_info<'a>(&'a self, ids: &'a [String]) -> ProviderFuture<'a, Vec<DownloadInfo>> {
        Box::pin(async move {
            Ok(self
                .drive
                .download_infos(ids)
                .await?
                .into_iter()
                .map(|info| DownloadInfo {
                    id: info.fid,
                    file_name: info.file_name,
                    size: info.size,
                    download_url: info.download_url,
                    headers: info.headers,
                })
                .collect())
        })
    }

    fn health(&self) -> ProviderFuture<'_, ProviderHealth> {
        Box::pin(async move {
            match self.drive.list_dir("0").await {
                Ok(_) => Ok(ProviderHealth {
                    healthy: true,
                    message: "夸克网盘连接正常".to_string(),
                }),
                Err(error) => Ok(ProviderHealth {
                    healthy: false,
                    message: error.to_string(),
                }),
            }
        })
    }
}
