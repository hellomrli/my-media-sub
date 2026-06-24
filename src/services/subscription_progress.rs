use crate::models::Subscription;
use crate::services::episode::detect_episode;

pub fn completion_target_episode(sub: &Subscription) -> Option<i32> {
    sub.rules
        .finish_after_episode
        .or(sub.total_episode_number)
        .filter(|episode| *episode > 0)
}

pub fn progress_max_episode(sub: &Subscription) -> i32 {
    let transferred_episodes = episode_numbers_from_file_names(sub.transferred_files.iter());
    sub.known_episodes
        .iter()
        .copied()
        .chain(transferred_episodes)
        .chain(std::iter::once(sub.current_episode_number))
        .max()
        .unwrap_or(0)
}

pub fn should_reopen_completed_subscription(sub: &Subscription) -> bool {
    if !sub.completed && sub.status != "completed" {
        return false;
    }

    completion_target_episode(sub)
        .map(|target| progress_max_episode(sub) < target)
        .unwrap_or(false)
}

pub fn reopen_completed_subscription_status(sub: &mut Subscription) -> bool {
    if !should_reopen_completed_subscription(sub) {
        return false;
    }

    sub.completed = false;
    sub.status = "active".to_string();
    sub.invalid_since = None;
    sub.last_error = String::new();
    true
}

pub fn episode_numbers_from_file_names<'a>(
    file_names: impl IntoIterator<Item = &'a String>,
) -> Vec<i32> {
    let mut episodes = file_names
        .into_iter()
        .filter_map(|name| detect_episode(name).episode)
        .collect::<Vec<_>>();
    episodes.sort();
    episodes.dedup();
    episodes
}

pub fn should_mark_completed_from_known_episodes(sub: &Subscription, new_episodes: &[i32]) -> bool {
    if sub.completed {
        return false;
    }

    let Some(target_episode) = completion_target_episode(sub) else {
        return false;
    };

    sub.known_episodes
        .iter()
        .chain(new_episodes.iter())
        .copied()
        .max()
        .map(|episode| episode >= target_episode)
        .unwrap_or(false)
}

pub fn should_mark_completed_from_transferred_files(
    sub: &Subscription,
    additional_file_names: &[String],
) -> bool {
    let mut file_names = sub.transferred_files.clone();
    file_names.extend(additional_file_names.iter().cloned());
    should_mark_completed_from_file_names(sub, &file_names)
}

pub fn should_mark_completed_from_file_names(sub: &Subscription, file_names: &[String]) -> bool {
    if sub.completed {
        return false;
    }

    let Some(target_episode) = completion_target_episode(sub) else {
        return false;
    };

    episode_numbers_from_file_names(file_names.iter())
        .into_iter()
        .max()
        .map(|episode| episode >= target_episode)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::rules::TransferRules;

    fn subscription() -> Subscription {
        Subscription {
            id: "sub1".to_string(),
            title: "Show".to_string(),
            source_title: String::new(),
            media_type: "series".to_string(),
            season: 1,
            start_episode_number: None,
            current_episode_number: 0,
            total_episode_number: Some(12),
            source_group: String::new(),
            metadata: None,
            cloud_type: "quark".to_string(),
            url: "https://pan.quark.cn/s/test".to_string(),
            password: String::new(),
            known_files: vec![],
            known_file_keys: vec![],
            known_episodes: vec![1, 2, 11],
            transferred_files: vec![],
            transferred_file_keys: vec![],
            last_probe: None,
            last_plan_summary: String::new(),
            notify_only: false,
            sync_download_enabled: false,
            sync_download_dir: String::new(),
            strm_enabled: false,
            enabled: true,
            completed: false,
            rules: TransferRules::default(),
            created_at: 0,
            updated_at: 0,
            last_checked_at: 0,
            last_new_files: vec![],
            last_new_episodes: vec![],
            last_check_summary: String::new(),
            check_history: vec![],
            status: "active".to_string(),
            invalid_since: None,
            last_error: String::new(),
            rule_summary: String::new(),
        }
    }

    #[test]
    fn test_should_mark_completed_from_known_episodes_uses_total_episode_number() {
        let sub = subscription();
        assert!(should_mark_completed_from_known_episodes(&sub, &[12]));
        assert!(!should_mark_completed_from_known_episodes(&sub, &[10]));
    }

    #[test]
    fn test_should_mark_completed_from_transferred_files() {
        let mut sub = subscription();
        sub.transferred_files = vec!["Show.S01E11.mkv".to_string()];

        assert!(should_mark_completed_from_transferred_files(
            &sub,
            &["Show.S01E12.mkv".to_string()]
        ));
        assert!(!should_mark_completed_from_transferred_files(
            &sub,
            &["Show.S01E10.mkv".to_string()]
        ));
    }

    #[test]
    fn test_should_reopen_completed_subscription_when_target_not_reached() {
        let mut sub = subscription();
        sub.completed = true;
        sub.status = "completed".to_string();
        sub.current_episode_number = 178;
        sub.total_episode_number = Some(190);
        sub.known_episodes = vec![177, 178];

        assert!(should_reopen_completed_subscription(&sub));

        sub.current_episode_number = 190;
        assert!(!should_reopen_completed_subscription(&sub));
    }

    #[test]
    fn test_reopen_completed_subscription_status_clears_completion_flags() {
        let mut sub = subscription();
        sub.completed = true;
        sub.status = "completed".to_string();
        sub.current_episode_number = 178;
        sub.total_episode_number = Some(190);
        sub.invalid_since = Some(1);
        sub.last_error = "completed".to_string();

        assert!(reopen_completed_subscription_status(&mut sub));
        assert!(!sub.completed);
        assert_eq!(sub.status, "active");
        assert_eq!(sub.invalid_since, None);
        assert!(sub.last_error.is_empty());
    }
}
