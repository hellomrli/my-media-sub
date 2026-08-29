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
            match crate::services::episode::cached_regex(match_regex) {
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
        // 已转存证据按「季+集」限定：known_episodes 是扁平集数列表（schema v1
        // 无季号），只对订阅主季生效；transferred_files 由文件名重新解析出
        // (season, episode)。否则多季订阅里 S02E01 会被 S01 的记录挡掉，
        // 第二季从此静默停更。
        let primary_season = sub.season.max(1);
        let transferred_episode_keys: HashSet<(i32, i32)> = if sub.media_type == "movie" {
            HashSet::new()
        } else {
            sub.transferred_files
                .iter()
                .filter_map(|name| episode_state_key_with_override(name, sub.season, &sub.rules.episode_regex))
                .collect()
        };

        let eligible_indices: Vec<usize> = files
            .iter()
            .enumerate()
            .filter_map(|(index, file)| {
                if sub.media_type != "movie" {
                    if let Some(key) = episode_state_key_with_override(&file.name, sub.season, &sub.rules.episode_regex) {
                        if transferred_episode_keys.contains(&key) {
                            // 同季同集已转存，跳过
                            return None;
                        }
                        if key.0 == primary_season && sub.known_episodes.contains(&key.1) {
                            // 主季集数已记录，跳过
                            return None;
                        }
                        // 单季订阅直接核对持久化的 `ep:N` 键（多集包拆单集
                        // 发布时，只有这里能识别出「该集已随合集转存」）；
                        // 多季订阅的旧 `ep:N` 键无季号语义，不参与判定。
                        if !sub.is_multi_season()
                            && sub.transferred_file_keys.contains(&format!("ep:{}", key.1))
                        {
                            return None;
                        }
                    }
                }

                (!file.is_dir
                    && Self::is_current_subscription_season_file(sub, file)
                    && !sub.known_file_keys.contains(&file.file_key)
                    && !self.is_before_start_episode(sub, &file.name, &file.parent_path)
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
        // 已转存证据按「季+集」解析（与 find_new_files 一致）；持久化的
        // `ep:N` 键不含季号，只对主季生效，避免多季订阅跨季误判。
        let transferred_episode_keys: HashSet<(i32, i32)> = sub
            .transferred_files
            .iter()
            .filter_map(|name| episode_state_key_with_override(name, sub.season, &sub.rules.episode_regex))
            .collect();
        let primary_season = sub.season.max(1);

        if sub.media_type == "movie" {
            // 电影没有集数概念，但 known 未转存的视频文件同样要补转：
            // 新文件判定只看 known_files，若首次转存提交失败，
            // 文件已入 known 集合，之后的检查必须靠这里重新入选。
            for file in files {
                if file.is_dir || !crate::services::is_video_name(&file.name) {
                    continue;
                }
                if self.transfer_rule_skip_reason(sub, file).is_some() {
                    continue;
                }
                let episode = extract_episode_number(&file.name);
                let key = transfer_state_key(&file.name, episode, sub.rules.ignore_extensions);
                if transferred_keys.contains(&key) {
                    continue;
                }
                if seen.insert(file.name.clone()) {
                    names.push(file.name.clone());
                }
            }
            return names;
        }

        for file in files {
            if file.is_dir || !Self::is_current_subscription_season_file(sub, file) {
                continue;
            }
            if self.is_before_start_episode(sub, &file.name, &file.parent_path) {
                continue;
            }
            if self.transfer_rule_skip_reason(sub, file).is_some() {
                continue;
            }
            let Some(key) = episode_state_key_with_override(&file.name, sub.season, &sub.rules.episode_regex) else {
                // 特典没有集数槽位：只按文件名键补转，避免占用正片集数。
                let episode = extract_episode_number(&file.name);
                let name_key =
                    transfer_state_key(&file.name, episode, sub.rules.ignore_extensions);
                if transferred_keys.contains(&name_key)
                    || sub.transferred_files.contains(&file.name)
                {
                    continue;
                }
                if seen.insert(file.name.clone()) {
                    names.push(file.name.clone());
                }
                continue;
            };
            if transferred_episode_keys.contains(&key) {
                continue;
            }
            if key.0 == primary_season && transferred_keys.contains(&format!("ep:{}", key.1)) {
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

        let key = episode_state_key_with_override(&file.name, sub.season, &sub.rules.episode_regex)?;
        // known_episodes 只承载主季语义：其他季的同号集数不受其约束。
        if key.0 == sub.season.max(1) && sub.known_episodes.contains(&key.1) {
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
            // 特典不参与同集择优分组：OVA02 与 EP02 不是同一集。
            let Some(key) = episode_state_key_with_override(&file.name, sub.season, &sub.rules.episode_regex) else {
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
        episode_state_key_with_override(&file.name, sub.season, &sub.rules.episode_regex)
            .map(|_| selected_episode_videos.contains(&index))
            .unwrap_or(true)
    }

    fn is_before_start_episode(
        &self,
        sub: &Subscription,
        file_name: &str,
        parent_path: &str,
    ) -> bool {
        if sub.media_type == "movie" {
            return false;
        }

        let Some(start_episode) = sub.start_episode_number else {
            return false;
        };
        if start_episode <= 1 {
            return false;
        }

        // 起始集数只约束订阅的起始季；后续季从第 1 集开始追，
        // 不能被上一季设置的起始集数整季过滤掉。
        let season = resolve_file_season(file_name, parent_path, sub.season, sub.is_multi_season());
        if season != Some(sub.season.max(1)) {
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
                    && !self.is_before_start_episode(sub, &file.name, &file.parent_path)
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
            } else if self.is_before_start_episode(sub, &file.name, &file.parent_path) {
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
            if !new_set.contains(file.name.as_str()) {
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
                episode_state_key_with_override(&file.name, sub.season, &sub.rules.episode_regex)
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
