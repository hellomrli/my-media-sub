use super::*;

pub const MAX_METADATA_TITLE_QUERIES: usize = 6;

static TITLE_PARTS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"《([^《》]+)》|「([^「」]+)」|『([^『』]+)』|\(([^()]+)\)|（([^（）]+)）")
        .expect("title alternatives regex")
});
static ALIAS_LABEL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:又名|别名|原名|译名)\s*[:：]\s*").expect("title alias label regex")
});
static ARC_TITLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(.+?)(?:\s+|之)[^\s]+(?:篇|篇章|章)$").expect("series arc title regex")
});

/// Keep the normal lookup first; the remaining names are fallbacks, not renames.
pub fn metadata_search_queries(
    query: &str,
    source_title: Option<&str>,
    media_type: Option<&str>,
) -> Vec<String> {
    let normalized = clean_media_title(query);
    if normalized.is_empty() || normalized.to_ascii_lowercase().starts_with("http") {
        return Vec::new();
    }
    let mut queries = vec![normalized.clone()];
    let mut sources = vec![query];
    if let Some(source) = source_title.filter(|source| {
        !source.trim().is_empty()
            && *source != query
            && title_identity(&clean_media_title(source)) == title_identity(&normalized)
    }) {
        sources.push(source);
    }

    // Recover discarded aliases, but never consult an unrelated source title
    // after the user has manually entered a different show name.
    for source in &sources {
        let prepared = pre_normalize(source);
        for captures in TITLE_PARTS_RE.captures_iter(&prepared).take(12) {
            if let Some(part) = captures.iter().skip(1).flatten().next() {
                push_query(&mut queries, &ALIAS_LABEL_RE.replace(part.as_str(), ""));
            }
        }
        let plain = strip_brackets_and_separators(
            &strip_trailing_description(&prepared),
            BracketPolicy::Drop,
        );
        for part in plain.split(['|', '/', '／']).take(12) {
            push_query(&mut queries, part);
        }
        if has_east_asian(&plain) {
            let mut latin = Vec::new();
            for token in plain.split_whitespace().chain(std::iter::once("")) {
                if !token.is_empty() && !has_east_asian(token) && !token.contains(['|', '/', '／'])
                {
                    latin.push(token);
                } else if !latin.is_empty() {
                    if latin.iter().any(|part| is_latin_segment(part)) {
                        push_query(&mut queries, &latin.join(" "));
                    }
                    latin.clear();
                }
            }
        }
    }

    // Replacing punctuation can expose a category prefix or glued quality tag.
    let spaced: String = normalized
        .chars()
        .map(|ch| if ch.is_alphanumeric() { ch } else { ' ' })
        .collect();
    push_query(&mut queries, &spaced);

    // Series arcs may lack separate TMDB records. Do not strip movie subtitles,
    // sequel numbers, or arbitrary words from titles without an explicit boundary.
    if matches!(media_type, Some("series" | "anime")) {
        let candidates = queries.clone();
        for candidate in candidates.iter().map(String::as_str).chain(sources) {
            if let Some(captures) = ARC_TITLE_RE.captures(candidate) {
                push_query(&mut queries, &captures[1]);
            }
            for separator in [":", "：", " - ", " — ", " – "] {
                if let Some((title, _)) = candidate.split_once(separator) {
                    push_query(&mut queries, title);
                }
            }
        }
    }
    if normalized.chars().any(is_cjk)
        && normalized
            .chars()
            .all(|ch| is_cjk(ch) || ch.is_ascii_digit() || ch.is_whitespace())
    {
        let compact: String = normalized
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect();
        push_query(&mut queries, &compact);
    }
    queries
}

fn push_query(queries: &mut Vec<String>, value: &str) {
    if queries.len() >= MAX_METADATA_TITLE_QUERIES {
        return;
    }
    let cleaned = clean_media_title(value);
    if cleaned.chars().filter(|ch| ch.is_alphanumeric()).count() < 2
        || cleaned.to_ascii_lowercase().starts_with("http")
        || NOISE_CORE_RE.is_match(&cleaned)
        || NOISE_TOKEN_EXTRA_RE.is_match(&cleaned)
        || queries
            .iter()
            .any(|query| query.to_lowercase() == cleaned.to_lowercase())
    {
        return;
    }
    queries.push(cleaned);
}

fn title_identity(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests;
