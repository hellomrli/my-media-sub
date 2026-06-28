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
}
