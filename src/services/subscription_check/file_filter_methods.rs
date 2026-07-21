macro_rules! subscription_check_file_filter_methods {
    () => {
    fn rule_display_name(name: &str, ignore_extensions: bool) -> String {
        if !ignore_extensions {
            return name.to_string();
        }

        std::path::Path::new(name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(name)
            .to_string()
    }

    fn normalized_rule_words(words: &[String]) -> Vec<String> {
        words
            .iter()
            .map(|word| word.trim().to_ascii_lowercase())
            .filter(|word| !word.is_empty())
            .collect()
    }

    fn contains_any_rule_word(value: &str, words: &[String]) -> bool {
        words.iter().any(|word| value.contains(word))
    }

    fn looks_like_derived_content(value: &str) -> bool {
        const CJK_DERIVED_KEYWORDS: &[&str] = &[
            "片头", "片尾", "片花", "插曲", "主题曲", "片尾曲", "片头曲", "花絮",
            "预告", "彩蛋", "特辑",
        ];
        if CJK_DERIVED_KEYWORDS
            .iter()
            .any(|keyword| value.contains(keyword))
        {
            return true;
        }

        value
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .filter(|token| !token.is_empty())
            .any(|token| {
                matches!(
                    token,
                    "mv" | "ost" | "op" | "ed" | "reaction" | "trailer" | "preview"
                ) || token.strip_prefix("op").is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit())
                }) || token.strip_prefix("ed").is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit())
                })
            })
    }


    fn transfer_rule_skip_reason(&self, sub: &Subscription, file: &ProbeFile) -> Option<String> {
        if !is_video_name(&file.name) {
            return Some("非视频文件".to_string());
        }

        let comparable = Self::rule_display_name(&file.name, sub.rules.ignore_extensions)
            .to_ascii_lowercase();

        if sub.media_type != "movie" && Self::looks_like_derived_content(&comparable) {
            return Some("疑似衍生内容".to_string());
        }

        if sub.media_type != "movie" {
            let Some((_, episode)) = episode_video_key(&file.name, sub.season) else {
                return Some("无法识别剧集集数".to_string());
            };
            if completion_target_episode(sub)
                .map(|target| episode > target)
                .unwrap_or(false)
            {
                return Some(format!("集数超过订阅总集数：第 {episode} 集"));
            }
        }

        let include_words = Self::normalized_rule_words(&sub.rules.include_keywords);
        if !include_words.is_empty() && !Self::contains_any_rule_word(&comparable, &include_words) {
            return Some("不含包含关键词".to_string());
        }

        let exclude_words = Self::normalized_rule_words(&sub.rules.exclude_keywords);
        if !exclude_words.is_empty() && Self::contains_any_rule_word(&comparable, &exclude_words) {
            return Some("命中排除关键词".to_string());
        }

        let match_regex = sub.rules.match_regex.trim();
        if !match_regex.is_empty() {
            match regex::Regex::new(match_regex) {
                Ok(re) if !re.is_match(&comparable) => {
                    return Some("未命中匹配正则".to_string());
                }
                Err(err) => return Some(format!("match_regex 无效：{}", err)),
                _ => {}
            }
        }

        None
    }

    /// 找出新增文件
    fn find_new_files(&self, sub: &Subscription, files: &[ProbeFile]) -> Vec<ProbeFile> {
        // 【新增】收集已转存的集数（包括 known_episodes 和 transferred_files）
        let mut known_episode_set = sub.known_episodes.iter().copied().collect::<HashSet<i32>>();

        // 从已转存文件名中提取集数，补充到 known_episode_set
        if sub.media_type != "movie" {
            for transferred_file in &sub.transferred_files {
                if let Some(key) = episode_video_key(transferred_file, sub.season) {
                    known_episode_set.insert(key.1);
                }
            }
        }

        let eligible_indices: Vec<usize> = files
            .iter()
            .enumerate()
            .filter_map(|(index, file)| {
                // 【修改】检查集数是否已在 known_episode_set 中
                if sub.media_type != "movie" {
                    if let Some(key) = episode_video_key(&file.name, sub.season) {
                        if known_episode_set.contains(&key.1) {
                            // 集数已转存，跳过
                            return None;
                        }
                    }
                }

                (!file.is_dir
                    && Self::is_current_subscription_season_file(sub, file)
                    && !sub.known_file_keys.contains(&file.file_key)
                    && !self.is_before_start_episode(sub, &file.name)
                    && self.transfer_rule_skip_reason(sub, file).is_none())
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
            if self.transfer_rule_skip_reason(sub, file).is_some() {
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
            || matches_subscription_season_range(
                &file.name,
                &file.parent_path,
                sub.season_start(),
                sub.season_end_inclusive(),
            )
    }

    fn should_record_known_probe_file(&self, sub: &Subscription, file: &ProbeFile) -> bool {
        !file.is_dir
            && Self::is_current_subscription_season_file(sub, file)
            && self.transfer_rule_skip_reason(sub, file).is_none()
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
                    && self.known_episode_video_reason(sub, file).is_none()
                    && self.transfer_rule_skip_reason(sub, file).is_none())
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
                ("skip", "非当前订阅季".to_string())
            } else if self.is_before_start_episode(sub, &file.name) {
                details.skipped_before_start_count += 1;
                ("skip", "低于起始转存集数".to_string())
            } else if let Some(reason) = self.transfer_rule_skip_reason(sub, file) {
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
