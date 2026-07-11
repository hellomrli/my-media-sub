use super::*;

pub(super) fn normalize_start_episode_number(value: Option<i32>, media_type: &str) -> Option<i32> {
    if media_type == "movie" {
        return None;
    }

    value.and_then(|episode| {
        let episode = episode.max(0);
        if episode > 0 {
            Some(episode)
        } else {
            None
        }
    })
}

pub(super) fn apply_source_change_options(
    sub: &mut Subscription,
    source_changed: bool,
    keep_progress: bool,
    continue_from_current: bool,
) {
    if !source_changed {
        return;
    }

    sub.status = "active".to_string();
    sub.invalid_since = None;
    sub.last_error = String::new();
    sub.completed = false;
    sub.last_probe = None;
    sub.last_new_files.clear();
    sub.last_new_episodes.clear();
    sub.last_check_summary = "已更换订阅资源，等待下次检查".to_string();

    if !keep_progress {
        sub.current_episode_number = 0;
        sub.known_files.clear();
        sub.known_file_keys.clear();
        sub.known_episodes.clear();
        sub.transferred_files.clear();
        sub.transferred_file_keys.clear();
        sub.start_episode_number = None;
        return;
    }

    if continue_from_current && sub.media_type != "movie" && sub.current_episode_number > 0 {
        sub.start_episode_number = Some(sub.current_episode_number + 1);
    }
}

pub(super) fn continue_from_current_episode_default(value: Option<bool>) -> bool {
    value.unwrap_or(true)
}

pub(super) fn reconcile_completion_status(sub: &mut Subscription) {
    reconcile_completed_subscription_status(sub);
}
