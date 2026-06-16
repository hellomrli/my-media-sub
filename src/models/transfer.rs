#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 转存计划
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferPlan {
    /// 总文件数
    pub total_count: i32,

    /// 匹配文件数
    pub matched_count: i32,

    /// 需转存文件数
    pub transfer_count: i32,

    /// 当前集数
    pub current_episode_number: i32,

    /// 已知集数列表
    pub episodes: Vec<i32>,

    /// 所有项目（包含匹配和未匹配）
    pub items: Vec<TransferItem>,

    /// 需转存的项目
    pub transfers: Vec<TransferItem>,

    /// 摘要
    pub summary: String,
}

/// 转存项目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferItem {
    /// 源文件名
    pub source_name: String,

    /// 文件 key
    #[serde(default)]
    pub file_key: String,

    /// 文件大小
    #[serde(default)]
    pub size: i64,

    /// 是否匹配
    #[serde(default)]
    pub matched: bool,

    /// 集数（可选）
    #[serde(default)]
    pub episode: Option<i32>,

    /// 目标目录
    #[serde(default)]
    pub target_dir: String,

    /// 目标文件名
    #[serde(default)]
    pub target_name: String,

    /// 是否需要转存
    #[serde(default)]
    pub should_transfer: bool,

    /// 其他字段（用于扩展）
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transfer_plan_serialize() {
        let plan = TransferPlan {
            total_count: 5,
            matched_count: 3,
            transfer_count: 2,
            current_episode_number: 3,
            episodes: vec![1, 2, 3],
            items: vec![TransferItem {
                source_name: "第01集.mkv".to_string(),
                file_key: "key1".to_string(),
                size: 123456789,
                matched: true,
                episode: Some(1),
                target_dir: "/动画/测试".to_string(),
                target_name: "测试.S01E01.mkv".to_string(),
                should_transfer: false,
                extra: HashMap::new(),
            }],
            transfers: vec![],
            summary: "匹配 3 个文件，规划新增 2 个".to_string(),
        };

        let json = serde_json::to_string_pretty(&plan).unwrap();
        println!("{}", json);

        // 验证能反序列化
        let _parsed: TransferPlan = serde_json::from_str(&json).unwrap();
    }
}
