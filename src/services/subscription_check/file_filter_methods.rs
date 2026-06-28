macro_rules! subscription_check_file_filter_methods {
    () => {
    /// 找出新增文件
    fn find_new_files(&self, sub: &Subscription, files: &[ProbeFile]) -> Vec<ProbeFile> {
        let eligible_indices: Vec<usize> = files
            .iter()
            .enumerate()
            .filter_map(|(index, file)| {
                (!file.is_dir
                    && Self::is_current_subscription_season_file(sub, file)
                    && !sub.known_file_keys.contains(&file.file_key)
                    && !self.is_before_start_episode(sub, &file.name)
                    && self.known_episode_video_reason(sub, file).is_none())
                .then_some(index)
            })
            .collect();
        let selected_episode_videos =
            self.selected_episode_video_indices(sub, files, &eligible_indices);

        eligible_indices
            .into_iter()
            .filter(|index| {
                self.keep_episode_video_index(sub, &files[*index], *index, &selected_episode_videos)
            })
            .map(|index| files[index].clone())
            .collect()
    }

    fn transfer_candidate_file_names(
        &self,
        sub: &Subscription,
        files: &[ProbeFile],
        new_file_names: &[String],
    ) -> Vec<String> {
        let mut names = new_file_names.to_vec();
        let mut seen = names.iter().cloned().collect::<HashSet<_>>();
        let mut transferred_keys: HashSet<String> =
            sub.transferred_file_keys.iter().cloned().collect();
        transferred_keys.extend(sub.transferred_files.iter().map(|name| {
            let episode = extract_episode_number(name);
            transfer_state_key(name, episode, sub.rules.ignore_extensions)
        }));

        if sub.media_type == "movie" {
            return names;
        }

        for file in files {
            if file.is_dir || !Self::is_current_subscription_season_file(sub, file) {
                continue;
            }
            if self.is_before_start_episode(sub, &file.name) {
                continue;
            }
            let episode = extract_episode_number(&file.name);
            let key = transfer_state_key(&file.name, episode, sub.rules.ignore_extensions);
            if !key.starts_with("ep:") || transferred_keys.contains(&key) {
                continue;
            }
            if seen.insert(file.name.clone()) {
                names.push(file.name.clone());
            }
        }

        names
    }

    fn known_episode_video_reason(
        &self,
        sub: &Subscription,
        file: &ProbeFile,
    ) -> Option<&'static str> {
        if sub.media_type == "movie" {
            return None;
        }

        let key = episode_video_key(&file.name, sub.season)?;
        let episode = key.1;
        if sub.known_episodes.contains(&episode) {
            return Some("同集已记录");
        }

        None
    }

    fn is_current_subscription_season_file(sub: &Subscription, file: &ProbeFile) -> bool {
        sub.media_type == "movie"
            || matches_subscription_season(&file.name, &file.parent_path, sub.season)
    }

    fn should_record_known_probe_file(sub: &Subscription, file: &ProbeFile) -> bool {
        !file.is_dir && Self::is_current_subscription_season_file(sub, file)
    }

    fn duplicate_episode_skip_reason(&self, sub: &Subscription) -> &'static str {
        match normalize_duplicate_episode_strategy(&sub.rules.duplicate_episode_strategy) {
            "latest_upload" => "同集重复视频，已保留上传时间最新版本",
            "largest_size" => "同集重复视频，已保留文件最大版本",
            "first" => "同集重复视频，已保留最先出现版本",
            _ => "同集重复视频，已保留清晰度最高版本",
        }
    }

    fn duplicate_candidate<'a>(
        &self,
        file: &'a ProbeFile,
        order: usize,
    ) -> EpisodeDuplicateCandidate<'a> {
        EpisodeDuplicateCandidate {
            name: &file.name,
            size: file.size,
            updated_at: file.updated_at.as_deref(),
            order,
        }
    }

    fn selected_episode_video_indices(
        &self,
        sub: &Subscription,
        files: &[ProbeFile],
        candidate_indices: &[usize],
    ) -> HashSet<usize> {
        if sub.media_type == "movie" {
            return HashSet::new();
        }

        let mut best_by_episode: HashMap<(i32, i32), usize> = HashMap::new();
        for &index in candidate_indices {
            let file = &files[index];
            if !Self::is_current_subscription_season_file(sub, file) {
                continue;
            }
            let Some(key) = episode_video_key(&file.name, sub.season) else {
                continue;
            };

            match best_by_episode.get(&key).copied() {
                Some(current_index) => {
                    if is_better_episode_duplicate_candidate(
                        self.duplicate_candidate(file, index),
                        self.duplicate_candidate(&files[current_index], current_index),
                        &sub.rules.duplicate_episode_strategy,
                    ) {
                        best_by_episode.insert(key, index);
                    }
                }
                None => {
                    best_by_episode.insert(key, index);
                }
            }
        }

        best_by_episode.values().copied().collect()
    }

    fn keep_episode_video_index(
        &self,
        sub: &Subscription,
        file: &ProbeFile,
        index: usize,
        selected_episode_videos: &HashSet<usize>,
    ) -> bool {
        if sub.media_type == "movie" {
            return true;
        }

        if !Self::is_current_subscription_season_file(sub, file) {
            return false;
        }

        episode_video_key(&file.name, sub.season)
            .map(|_| selected_episode_videos.contains(&index))
            .unwrap_or(true)
    }

    fn is_before_start_episode(&self, sub: &Subscription, file_name: &str) -> bool {
        if sub.media_type == "movie" {
            return false;
        }

        let Some(start_episode) = sub.start_episode_number else {
            return false;
        };
        if start_episode <= 1 {
            return false;
        }

        extract_episode_number(file_name)
            .map(|episode| episode < start_episode)
            .unwrap_or(false)
    }

    fn build_check_details(&self, sub: &Subscription, files: &[ProbeFile]) -> CheckDetails {
        let mut details = CheckDetails {
            scanned_count: files.len(),
            ..Default::default()
        };

        let detail_candidate_indices: Vec<usize> = files
            .iter()
            .enumerate()
            .filter_map(|(index, file)| {
                (!file.is_dir
                    && Self::is_current_subscription_season_file(sub, file)
                    && !sub.known_file_keys.contains(&file.file_key)
                    && !self.is_before_start_episode(sub, &file.name)
                    && self.known_episode_video_reason(sub, file).is_none())
                .then_some(index)
            })
            .collect();
        let selected_episode_videos =
            self.selected_episode_video_indices(sub, files, &detail_candidate_indices);

        for (index, file) in files.iter().enumerate() {
            let episode = extract_episode_number(&file.name);
            let (action, reason) = if file.is_dir {
                details.skipped_directory_count += 1;
                ("skip", "目录不参与订阅检查")
            } else if sub.known_file_keys.contains(&file.file_key) {
                details.known_count += 1;
                ("known", "已知文件")
            } else if !Self::is_current_subscription_season_file(sub, file) {
                details.skipped_other_season_count += 1;
                ("skip", "非当前订阅季")
            } else if self.is_before_start_episode(sub, &file.name) {
                details.skipped_before_start_count += 1;
                ("skip", "低于起始转存集数")
            } else if let Some(reason) = self.known_episode_video_reason(sub, file) {
                details.skipped_duplicate_episode_count += 1;
                ("skip", reason)
            } else if !self.keep_episode_video_index(sub, file, index, &selected_episode_videos) {
                details.skipped_duplicate_episode_count += 1;
                ("skip", self.duplicate_episode_skip_reason(sub))
            } else {
                details.new_count += 1;
                ("new", "新增文件")
            };

            details.items.push(CheckDetailItem {
                name: file.name.clone(),
                episode,
                is_dir: file.is_dir,
                parent_path: file.parent_path.clone(),
                file_key: file.file_key.clone(),
                action: action.to_string(),
                reason: reason.to_string(),
            });
        }

        details
    }

    /// 解析集数
    fn parse_episodes(&self, file_names: &[String]) -> Vec<i32> {
        let mut episodes = Vec::new();

        for name in file_names {
            if let Some(ep) = extract_episode_number(name) {
                if !episodes.contains(&ep) {
                    episodes.push(ep);
                }
            }
        }

        episodes.sort();
        episodes
    }

    };
}
