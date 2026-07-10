/// 转存结果
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TransferResult {
    pub subscription_id: String,
    pub transferred_count: usize,
    pub skipped: bool,
    pub reason: String,
    pub push_title: Option<String>,
    pub push_message: Option<String>,
    pub push_notification_id: Option<String>,
    pub renamed_count: usize,
    pub strm_generated_count: usize,
    pub strm_error: Option<String>,
    pub aria2_submitted_count: usize,
    pub aria2_error: Option<String>,
}
