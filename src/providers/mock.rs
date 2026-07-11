use super::*;
use std::collections::HashMap;
use std::sync::Mutex;

/// Deterministic in-memory provider for service tests.
pub struct MockCloudDriveProvider {
    cloud_type: &'static str,
    probe_result: Mutex<ProviderProbeResult>,
    items: Mutex<HashMap<String, Vec<DriveItem>>>,
    failures: Mutex<HashMap<&'static str, String>>,
    transfers: Mutex<Vec<TransferRequest>>,
}

impl Default for MockCloudDriveProvider {
    fn default() -> Self {
        Self {
            cloud_type: "mock",
            probe_result: Mutex::new(ProviderProbeResult {
                ok: true,
                state: "ok".to_string(),
                message: "mock ok".to_string(),
                files: vec![],
            }),
            items: Mutex::new(HashMap::new()),
            failures: Mutex::new(HashMap::new()),
            transfers: Mutex::new(vec![]),
        }
    }
}

impl MockCloudDriveProvider {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_probe_result(&self, result: ProviderProbeResult) {
        *self.probe_result.lock().expect("mock probe lock") = result;
    }
    pub fn set_items(&self, parent_id: impl Into<String>, items: Vec<DriveItem>) {
        self.items
            .lock()
            .expect("mock items lock")
            .insert(parent_id.into(), items);
    }
    pub fn fail(&self, operation: &'static str, message: impl Into<String>) {
        self.failures
            .lock()
            .expect("mock failures lock")
            .insert(operation, message.into());
    }
    pub fn transfer_requests(&self) -> Vec<TransferRequest> {
        self.transfers.lock().expect("mock transfers lock").clone()
    }
    fn error(&self, operation: &'static str) -> Result<()> {
        match self
            .failures
            .lock()
            .expect("mock failures lock")
            .get(operation)
            .cloned()
        {
            Some(message) => Err(AppError::Http(message)),
            None => Ok(()),
        }
    }
}

impl CloudDriveProvider for MockCloudDriveProvider {
    fn cloud_type(&self) -> &'static str {
        self.cloud_type
    }
    fn probe<'a>(
        &'a self,
        _url: &'a str,
        _passcode: &'a str,
        _max_files: usize,
    ) -> ProviderFuture<'a, ProviderProbeResult> {
        Box::pin(async move {
            self.error("probe")?;
            Ok(self.probe_result.lock().expect("mock probe lock").clone())
        })
    }
    fn list<'a>(&'a self, parent_id: &'a str) -> ProviderFuture<'a, Vec<DriveItem>> {
        Box::pin(async move {
            self.error("list")?;
            Ok(self
                .items
                .lock()
                .expect("mock items lock")
                .get(parent_id)
                .cloned()
                .unwrap_or_default())
        })
    }
    fn find<'a>(
        &'a self,
        parent_id: &'a str,
        name: &'a str,
    ) -> ProviderFuture<'a, Option<DriveItem>> {
        Box::pin(async move {
            self.error("find")?;
            Ok(self
                .items
                .lock()
                .expect("mock items lock")
                .get(parent_id)
                .and_then(|items| items.iter().find(|item| item.name == name).cloned()))
        })
    }
    fn ensure<'a>(&'a self, path: &'a str) -> ProviderFuture<'a, String> {
        Box::pin(async move {
            self.error("ensure")?;
            Ok(format!("mock:{}", path.trim_matches('/')))
        })
    }
    fn transfer<'a>(&'a self, request: TransferRequest) -> ProviderFuture<'a, TransferOutcome> {
        Box::pin(async move {
            self.error("transfer")?;
            self.transfers
                .lock()
                .expect("mock transfers lock")
                .push(request.clone());
            let selected: std::collections::HashSet<_> = request.file_ids.iter().collect();
            let files = self
                .probe_result
                .lock()
                .expect("mock probe lock")
                .files
                .iter()
                .filter(|file| !file.is_dir && (selected.is_empty() || selected.contains(&file.id)))
                .cloned()
                .collect();
            Ok(TransferOutcome {
                transferred_files: files,
            })
        })
    }
    fn rename<'a>(
        &'a self,
        _id: &'a str,
        _name: &'a str,
        _parent_id: Option<&'a str>,
    ) -> ProviderFuture<'a, ()> {
        Box::pin(async move { self.error("rename") })
    }
    fn delete<'a>(&'a self, _ids: &'a [String]) -> ProviderFuture<'a, ()> {
        Box::pin(async move { self.error("delete") })
    }
    fn download_info<'a>(&'a self, ids: &'a [String]) -> ProviderFuture<'a, Vec<DownloadInfo>> {
        Box::pin(async move {
            self.error("download_info")?;
            Ok(ids
                .iter()
                .map(|id| DownloadInfo {
                    id: id.clone(),
                    file_name: id.clone(),
                    size: 0,
                    download_url: format!("mock://{id}"),
                    headers: vec![],
                })
                .collect())
        })
    }
    fn health(&self) -> ProviderFuture<'_, ProviderHealth> {
        Box::pin(async move {
            self.error("health")?;
            Ok(ProviderHealth {
                healthy: true,
                message: "mock ok".to_string(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(id: &str) -> ProviderFile {
        ProviderFile {
            id: id.into(),
            name: format!("{id}.mkv"),
            is_dir: false,
            size: 1,
            parent_path: String::new(),
            updated_at: None,
        }
    }

    #[tokio::test]
    async fn supports_probe_and_selected_transfer() {
        let provider = MockCloudDriveProvider::new();
        provider.set_probe_result(ProviderProbeResult {
            ok: true,
            state: "ok".into(),
            message: String::new(),
            files: vec![file("a"), file("b")],
        });
        let probe = provider.probe("mock://share", "", 10).await.unwrap();
        assert_eq!(probe.files.len(), 2);
        let outcome = provider
            .transfer(TransferRequest {
                share_url: "mock://share".into(),
                passcode: String::new(),
                target_id: "root".into(),
                file_ids: vec!["b".into()],
            })
            .await
            .unwrap();
        assert_eq!(outcome.transferred_files, vec![file("b")]);
        assert_eq!(provider.transfer_requests().len(), 1);
    }

    #[tokio::test]
    async fn injects_operation_failures() {
        let provider = MockCloudDriveProvider::new();
        provider.fail("probe", "upstream unavailable");
        let error = provider.probe("mock://share", "", 10).await.unwrap_err();
        assert!(error.to_string().contains("upstream unavailable"));
    }
}
