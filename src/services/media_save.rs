use std::collections::HashSet;

use crate::error::{AppError, Result};
use crate::models::MediaSaveOptions;
use crate::providers::{DriveItem, ProviderFile};
use crate::services::media_selection::{is_main_video, matches_resolution};

pub fn select_save_files(
    files: &[ProviderFile],
    options: &MediaSaveOptions,
) -> Result<Vec<ProviderFile>> {
    let requested: HashSet<&str> = options.file_ids.iter().map(String::as_str).collect();
    let selected = files
        .iter()
        .filter(|file| {
            !file.is_dir
                && !file.id.is_empty()
                && is_main_video(&file.name)
                && matches_resolution(&options.preferred_resolution, &file.name, &file.parent_path)
                && (requested.is_empty() || requested.contains(file.id.as_str()))
        })
        .cloned()
        .collect::<Vec<_>>();
    if !requested.is_empty() && selected.len() != requested.len() {
        return Err(AppError::Validation(
            "所选资源已变化或画质不匹配，请重新嗅探后选择".to_string(),
        ));
    }
    if selected.is_empty() {
        return Err(AppError::Validation(
            "没有找到所选画质的正片，不会回退保存其他版本".to_string(),
        ));
    }
    if options.media_type == "movie" && selected.len() != 1 {
        return Err(AppError::Validation(
            "电影有多个正片版本，请先嗅探并选择一个文件".to_string(),
        ));
    }
    let mut names = HashSet::new();
    if selected
        .iter()
        .any(|file| !names.insert(file.name.as_str()))
    {
        return Err(AppError::Validation(
            "所选资源包含同名文件，请缩小选择范围或使用剧集追更按季保存".to_string(),
        ));
    }
    Ok(selected)
}

pub fn resolve_saved_files(
    expected: &[ProviderFile],
    items: &[DriveItem],
) -> Result<Vec<ProviderFile>> {
    expected
        .iter()
        .map(|source| {
            let mut matches = items
                .iter()
                .filter(|item| !item.is_dir && item.name == source.name);
            let saved = matches.next().ok_or_else(|| {
                AppError::Http(format!("网盘尚未返回已保存文件：{}", source.name))
            })?;
            if matches.next().is_some() || saved.id.is_empty() {
                return Err(AppError::Validation(format!(
                    "已保存文件存在歧义，未提交下载：{}",
                    source.name
                )));
            }
            if source.size > 0 && saved.size != source.size {
                return Err(AppError::Validation(format!(
                    "已保存文件大小与所选版本不符，未提交下载：{}",
                    source.name
                )));
            }
            Ok(ProviderFile {
                id: saved.id.clone(),
                name: saved.name.clone(),
                is_dir: false,
                size: saved.size,
                parent_path: String::new(),
                updated_at: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(id: &str, folder: &str) -> ProviderFile {
        ProviderFile {
            id: id.to_string(),
            name: "Movie.mkv".to_string(),
            parent_path: folder.to_string(),
            size: 10,
            is_dir: false,
            updated_at: None,
        }
    }

    #[test]
    fn movie_selects_exact_file_and_rejects_missing_or_wrong_quality() {
        let files = vec![file("hd", "1080P"), file("uhd", "4K")];
        let mut options = MediaSaveOptions {
            media_type: "movie".to_string(),
            preferred_resolution: "1080p".to_string(),
            file_ids: vec!["hd".to_string()],
            ..Default::default()
        };
        assert_eq!(select_save_files(&files, &options).unwrap()[0].id, "hd");
        options.file_ids = vec!["uhd".to_string()];
        assert!(select_save_files(&files, &options).is_err());
        options.file_ids = vec!["missing".to_string()];
        assert!(select_save_files(&files, &options).is_err());
        options.file_ids.clear();
        options.preferred_resolution.clear();
        assert!(select_save_files(&files, &options).is_err());
    }

    #[test]
    fn saved_files_use_own_drive_ids_not_share_ids() {
        let items = vec![DriveItem {
            id: "own-drive-id".to_string(),
            parent_id: "target".to_string(),
            name: "Movie.mkv".to_string(),
            is_dir: false,
            size: 10,
            updated_at: String::new(),
        }];
        let resolved = resolve_saved_files(&[file("share-id", "4K")], &items).unwrap();
        assert_eq!(resolved[0].id, "own-drive-id");
        assert!(resolve_saved_files(&[file("share-id", "4K")], &[]).is_err());
        assert!(resolve_saved_files(
            &[file("share-id", "4K")],
            &[items[0].clone(), items[0].clone()]
        )
        .is_err());
        let mut wrong_version = items[0].clone();
        wrong_version.size = 20;
        assert!(resolve_saved_files(&[file("share-id", "4K")], &[wrong_version]).is_err());
    }
}
