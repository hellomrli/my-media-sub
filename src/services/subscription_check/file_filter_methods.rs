macro_rules! subscription_check_file_filter_methods {
    () => {
    /// 找出新增文件
    fn find_new_files(&self, sub: &Subscription, files: &[ProbeFile]) -> Vec<ProbeFile> {
        // 已转存证据按「季+集」限定：known_episodes 是扁平集数列表（schema v1
        // 无季号），只对订阅主季生效；transferred_files 由文件名重新解析出
        // (season, episode)。否则多季订阅里 S02E01 会被 S01 的记录挡掉，
        // 第二季从此静默停更。
        let primary_season = sub.season.max(1);
        let transferred_episode_keys = crate::services::episode::transferred_episode_keys(sub);

        let eligible_indices: Vec<usize> = files
            .iter()
            .enumerate()
            .filter_map(|(index, file)| {
                if sub.media_type != "movie" {
                    if let Some(key) = episode_state_key_with_context(&file.name, &file.parent_path, sub.season, &sub.rules.episode_regex) {
                        if transferred_episode_keys.contains(&key) {
                            // 同季同集已转存，跳过
                            return None;
                        }
                        if key.0 == primary_season && sub.known_episodes.contains(&key.1) {
                            // 主季集数已记录，跳过
                            return None;
                        }

                    }
                }

                (!file.is_dir
                    && Self::is_current_subscription_season_file(sub, file)
                    && !sub.known_file_keys.contains(&file.file_key)
                    && !is_before_start_episode(sub, &file.name, &file.parent_path)
                    && transfer_rule_skip_reason(sub, &file.name, &file.parent_path).is_none())
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
    ) -> Vec<String> {
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        let mut transferred_keys: HashSet<String> = sub.transferred_file_keys.iter().cloned().collect();
        if sub.media_type == "movie" {
            transferred_keys.extend(sub.transferred_files.iter().map(|name| {
                transfer_state_key(name, extract_episode_number(name), sub.rules.ignore_extensions)
            }));
        }
        let transferred_episode_keys = crate::services::episode::transferred_episode_keys(sub);

        if sub.media_type == "movie" {
            // 电影没有集数概念，但 known 未转存的视频文件同样要补转：
            // 新文件判定只看 known_files，若首次转存提交失败，
            // 文件已入 known 集合，之后的检查必须靠这里重新入选。
            for file in files {
                if file.is_dir || !crate::services::is_video_name(&file.name) {
                    continue;
                }
                if transfer_rule_skip_reason(sub, &file.name, &file.parent_path).is_some() {
                    continue;
                }
                let episode = extract_episode_number(&file.name);
                let key = transfer_state_key(&file.name, episode, sub.rules.ignore_extensions);
                if transferred_keys.contains(&key) {
                    continue;
                }
                let reference = crate::services::episode::file_reference(&file.name, &file.parent_path);
                if seen.insert(reference.clone()) {
                    names.push(reference);
                }
            }
            return names;
        }

        for file in files {
            if file.is_dir || !Self::is_current_subscription_season_file(sub, file) {
                continue;
            }
            if is_before_start_episode(sub, &file.name, &file.parent_path) {
                continue;
            }
            if transfer_rule_skip_reason(sub, &file.name, &file.parent_path).is_some() {
                continue;
            }
            let Some(key) = episode_state_key_with_context(&file.name, &file.parent_path, sub.season, &sub.rules.episode_regex) else {
                // 特典没有集数槽位：只按文件名键补转，避免占用正片集数。
                let name_key = transfer_state_key(&file.name, None, sub.rules.ignore_extensions);
                let season = resolve_file_season(&file.name, &file.parent_path, sub.season, sub.is_multi_season()).unwrap_or(sub.season);
                let saved_reference = crate::services::episode::progress_file_reference(&file.name, &file.parent_path, season);
                if transferred_keys.contains(&format!("s:{season}:{name_key}"))
                    || (season == sub.season && transferred_keys.contains(&name_key))
                    || sub.transferred_files.iter().any(|name| crate::services::episode::progress_file_reference(name, "", sub.season) == saved_reference)
                {
                    continue;
                }
                let reference = crate::services::episode::file_reference(&file.name, &file.parent_path);
                if seen.insert(reference.clone()) {
                    names.push(reference);
                }
                continue;
            };
            if transferred_episode_keys.contains(&key) {
                continue;
            }
            let reference = crate::services::episode::file_reference(&file.name, &file.parent_path);
            if seen.insert(reference.clone()) {
                names.push(reference);
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

        let key = episode_state_key_with_context(&file.name, &file.parent_path, sub.season, &sub.rules.episode_regex)?;
        // known_episodes 只承载主季语义：其他季的同号集数不受其约束。
        if key.0 == sub.season.max(1) && sub.known_episodes.contains(&key.1) {
            return Some("同集已记录");
        }

        None
    }

    fn is_current_subscription_season_file(sub: &Subscription, file: &ProbeFile) -> bool {
        crate::services::episode::subscription_file_matches_season(sub, &file.name, &file.parent_path)
    }

    fn should_record_known_probe_file(&self, sub: &Subscription, file: &ProbeFile) -> bool {
        !file.is_dir
            && Self::is_current_subscription_season_file(sub, file)
            && transfer_rule_skip_reason(sub, &file.name, &file.parent_path).is_none()
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
            // 特典不参与同集择优分组：OVA02 与 EP02 不是同一集。
            let Some(key) = episode_state_key_with_context(&file.name, &file.parent_path, sub.season, &sub.rules.episode_regex) else {
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

        // 特典没有集数槽位，不参与同集择优（见 selected_episode_video_indices）。
        episode_state_key_with_context(&file.name, &file.parent_path, sub.season, &sub.rules.episode_regex)
            .map(|_| selected_episode_videos.contains(&index))
            .unwrap_or(true)
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
                    && !is_before_start_episode(sub, &file.name, &file.parent_path)
                    && self.known_episode_video_reason(sub, file).is_none()
                    && transfer_rule_skip_reason(sub, &file.name, &file.parent_path).is_none())
                .then_some(index)
            })
            .collect();
        let selected_episode_videos =
            self.selected_episode_video_indices(sub, files, &detail_candidate_indices);

        for (index, file) in files.iter().enumerate() {
            let detection = crate::services::episode::detect_episode_with_override(
                &file.name, &sub.rules.episode_regex,
            ).unwrap_or_else(|error| {
                let mut fallback = crate::services::episode::detect_episode_explained(&file.name);
                fallback.reason = error;
                fallback
            });
            let episode = detection.episode;
            let (action, reason) = if file.is_dir {
                details.skipped_directory_count += 1;
                ("skip", "目录不参与订阅检查".to_string())
            } else if sub.known_file_keys.contains(&file.file_key) {
                details.known_count += 1;
                ("known", "已知文件".to_string())
            } else if !Self::is_current_subscription_season_file(sub, file) {
                details.skipped_other_season_count += 1;
                ("skip", "不在订阅季度，暂不转存；编辑订阅勾选该季即可转存".to_string())
            } else if is_before_start_episode(sub, &file.name, &file.parent_path) {
                details.skipped_before_start_count += 1;
                ("skip", "低于起始转存集数".to_string())
            } else if let Some(reason) = transfer_rule_skip_reason(sub, &file.name, &file.parent_path) {
                ("skip", reason.to_string())
            } else if let Some(reason) = self.known_episode_video_reason(sub, file) {
                details.skipped_duplicate_episode_count += 1;
                ("skip", reason.to_string())
            } else if !self.keep_episode_video_index(sub, file, index, &selected_episode_videos) {
                details.skipped_duplicate_episode_count += 1;
                ("skip", self.duplicate_episode_skip_reason(sub).to_string())
            } else {
                details.new_count += 1;
                ("new", "新增文件".to_string())
            };

            details.items.push(CheckDetailItem {
                name: file.name.clone(),
                episode,
                episodes: detection.episodes.clone(),
                special_kind: detection.special_kind.map(str::to_string),
                detection_method: detection.method.to_string(),
                detection_confidence: detection.confidence.to_string(),
                is_dir: file.is_dir,
                parent_path: file.parent_path.clone(),
                file_key: file.file_key.clone(),
                action: action.to_string(),
                reason,
            });
        }

        details
    }

    /// 解析集数（主链路证据提取）。
    ///
    /// 与检查识别使用同一 override 语义；特典（SP/OVA/OAD）不产出集数证据，
    /// 避免以 ep:N 的身份占用正片集数槽位。主季过滤的写入口径见
    /// `primary_season_new_episodes`。
    fn parse_episodes(&self, sub: &Subscription, file_names: &[String]) -> Vec<i32> {
        let mut episodes = Vec::new();

        for name in file_names {
            let Some((_, episode)) =
                episode_state_key_with_override(name, sub.season, &sub.rules.episode_regex)
            else {
                continue;
            };
            if !episodes.contains(&episode) {
                episodes.push(episode);
            }
        }

        episodes.sort();
        episodes
    }

    /// 从新增文件中提取「主季」集数，用于写入口径。
    ///
    /// `known_episodes` 是扁平集数列表（schema v1 无季号），只承载主季语义：
    /// 多季订阅里其他季的集数不得写入，否则 S02 的集数会在后续检查中
    /// 跨季误挡 S01 的同号文件。
    fn primary_season_new_episodes(
        &self,
        sub: &Subscription,
        probe_files: &[ProbeFile],
        new_files: &[String],
    ) -> Vec<i32> {
        let primary = sub.season.max(1);
        let new_set: HashSet<&str> = new_files.iter().map(String::as_str).collect();
        let mut episodes = Vec::new();
        for file in probe_files {
            let reference = crate::services::episode::file_reference(&file.name, &file.parent_path);
            if !new_set.contains(reference.as_str()) && !new_set.contains(file.name.as_str()) {
                continue;
            }
            let season = resolve_file_season(
                &file.name,
                &file.parent_path,
                sub.season,
                sub.is_multi_season(),
            );
            if season != Some(primary) {
                continue;
            }
            let Some((_, episode)) =
                episode_state_key_with_context(&file.name, &file.parent_path, sub.season, &sub.rules.episode_regex)
            else {
                continue;
            };
            if !episodes.contains(&episode) {
                episodes.push(episode);
            }
        }
        episodes.sort();
        episodes
    }

    };
}
