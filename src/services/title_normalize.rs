//! 分享标题清洗：剥离字幕组/画质/季标噪声，供 TMDB 匹配与订阅命名共用。
//!
//! 清洗分几步，顺序有依赖：
//!
//! 0. **预归一化**：全角字母数字转半角、ideographic space 转普通空格，并把
//!    `DDP5.1` / `H.264` / `x.265` 这类**自带点号的技术词**先粘合起来——否则下一步
//!    把 `.` 换成空格后，`5.1` 会拆成 `5` 与 `1`，而尾部一个孤零零的 `1` 不是噪声，
//!    会阻断前面所有噪声的剥离。
//! 1. **提取提示**：在丢弃任何内容之前先记下年份（`(2024)`、`.2024.`）与季号
//!    （`第二季`、`S02`、`Season 2`、`S01-S04`）——它们对元数据匹配与订阅表单
//!    都有用，但对搜索关键词是噪声。
//! 2. **去括号**：`[]`、`【】`、`()`、`（）` 内的内容整体丢弃（几乎总是字幕组、
//!    画质或年份）；`《》`、`「」`、`『』` 是**书名号**，里面的正是标题，只去掉符号。
//!    若丢弃括号内容后什么都不剩（`【庆余年】1080p`），回退为保留括号内容再清洗。
//! 3. **双语选择**：中日/中英并列时挑出更利于中文元数据匹配的那一段。
//! 4. **季集/进度尾缀**：这类标记常含空格（`Season 1`、`全 24 集`），需在整串上处理。
//! 5. **噪声 token 清洗**：从尾部逐个 token 剥离已知噪声，遇到第一个非噪声 token 停止。
//!
//! 第 5 步必须按 token 逐个判断，而不能只做「整段尾缀必须全部匹配」的正则替换：
//! `信号 1080p 韩语中字` 这类标题里 `韩语` 若不在词表中，整段匹配会失败，从而把
//! 前面的 `1080p` 一起留在标题里；逐个剥离则能一路清到 `信号`。

use regex::Regex;
use std::sync::LazyLock;

/// 噪声词条：**在 token 内部作为尾缀也安全**的词。
///
/// 收录标准是「自带数字、CJK 字符，或是足够长的技术缩写」，因此不会误伤真实剧名：
/// 不能收录 `ts`（会命中 `Robots`）、`ma`（会命中 `Drama`）这类短后缀，
/// 它们只允许作为完整 token 匹配，见 [`NOISE_TOKEN_EXTRA`]。
const NOISE_CORE: &str = r"(?ix)
    # 分辨率 / 动态范围 / 帧率
    \d{3,4}[pi] | [248]k | uhd | fhd | hdr10\+? | hdr | sdr | dovi | hlg | \d+\s*帧 | \d+fps |
    # 来源 / 编码 / 音频
    web[\s-]?dl | web[\s-]?rip | blu[\s-]?ray | bdrip | brrip | hdtv | tvrip | dvdrip | hdrip |
    remux | h\.?26[45] | x26[45] | hevc | avc | av1 | vp9 |
    10bits? | 8bits? | ma10p | hi10p | e-?ac-?3 | dd\+ | hi-?fi | hi-?res |
    # 音频编码可带声道数（`AAC2.0`、`DDP5.1`、`TrueHD7.1`、`DDP2`）
    (?:aac|ac3|eac3|ddp?|dts(?:-?hd)?(?:-?ma)?|flac|truehd|atmos|opus)(?:\d(?:\.?\d)?)? |
    # 语言 / 字幕
    国粤双语 | 国语中字 | 中文字幕 | 内嵌字幕 | 外挂字幕 | 简繁外挂 | 简繁内封 | 简繁中字 |
    简繁双语 | 简繁 | 简中 | 繁中 | 简体中字 | 繁体中字 | 中英双字 | 中日双字 | 中英字幕 |
    双字幕 | 双字 | 双语 | 国语 | 国配 | 粤语 | 中配 | 台配 | 日配 | 英配 | 韩语 | 日语 | 英语 |
    原声 | 中字 | 字幕 | 内嵌 | 外挂 | 内封 | 硬字幕 | 软字幕 | 生肉 | 熟肉 | 字幕组 | 压制 |
    # 版本 / 画质形容
    导演剪辑版 | 未删减版 | 未删减 | 删减版 | 修复版 | 重制版 | 高清版 | 完整版 | 纯净版 |
    收藏版 | 加长版 | 特别版 | 纪念版 | 精简版 | 合集版 | 先行版 | 抢先版 | 枪版 |
    国语版 | 粤语版 | 中配版 | 台配版 | 日配版 | 英配版 | 配音版 | 原版 |
    高码率 | 低码率 | 高帧率 | 超高清 | 高码 | 低码 | 高帧 |
    杜比视界 | 杜比全景声 | 杜比 | dolby | 无损 | 原盘 | 蓝光 | 高清 | 标清 |
    豆瓣\s*\d+(?:\.\d+)?\s*分? | \d+(?:\.\d+)?\s*分 |
    # 季 / 集 / 进度 / 分类
    第\s*[0-9一二三四五六七八九十百两]+\s*[季期部] |
    第\s*[0-9一二三四五六七八九十百两]+\s*[集话話回] |
    全\s*\d+\s*[集话話回] | 共\s*\d+\s*[集话話回] | 全\s*\d+\s*季 | \d+\s*[集话話回] |
    \d{1,3}\s*[-~～]\s*\d{1,3}\s*[集话話回] |
    s\d{1,2}(?:\s*[-~～到至]\s*s?\d{1,2})? | season\s*\d+(?:\s*[-~～到至]\s*\d+)? |
    s\d{1,2}e\d{1,3}(?:[-~]e?\d{1,3})? |
    (?:19|20)\d{2}\s*年 |
    全集 | 合集 | 完结 | 已完结 | 完结篇 | 最终季 | 最终章 | 最终话 | 连载 | 连载中 |
    更新中 | 更新至.* | 更至.* | 持续更新 | 正在更新 | 周更 | 日更 | 年番 | 番外 |
    上下部 | 上部 | 下部 | 上篇 | 下篇 | 前篇 | 后篇 |
    剧场版 | 电视剧 | 电影版 | 剧集版 | 真人版 | 动画版 | tv版 | tv动画 | 电视动画 |
    纪录片 | 综艺 | 国漫 | 日漫 | 美剧 | 英剧 | 韩剧 | 日剧 | 港剧 | 台剧 | 泰剧 | 国产剧 |
    热播 | 热门 | 独播 | 首播 | 抢先看 | 抢先 | 新番 | 正片 | 花絮 | 预告 |
    imax | 3d | 4d | 2d
";

/// 只能作为**完整 token** 才安全清除的短词。
///
/// `ts` / `ma` / `hd` 这类两字母词一旦允许尾缀匹配，就会把 `Robots`、`Drama`
/// 之类的真实剧名切掉尾巴。`\d{3,4}` 与 4 位年份同理：必须是独立 token，
/// 这样 `请回答1988`（年份与剧名粘连）不会被误伤。
/// 平台 / 片源名：既可作为尾缀 token 清除，也允许出现在标题**开头**
/// （`Netflix 鱿鱼游戏`）时被剥掉。
const NOISE_PLATFORMS: &str = r"(?ix)
    netflix | nf | hbo | hbomax | hulu | amzn | amazon | disney\+? | dsnp | atvp | appletv |
    peacock | pcok | paramount\+? | pmtp | crunchyroll | cr | iqiyi | iq | youku | tencent |
    bilibili | b站 | 爱奇艺 | 优酷 | 腾讯 | 芒果tv | 芒果 | 哔哩哔哩 | 央视 | cctv\d*
";

const NOISE_TOKEN_EXTRA: &str = r"(?ix)
    hd | sd | fhd | uhd | hq | dl | web | webdl | bd | br | bdr | ts | tc | hc | cam |
    sp | ma | dv | ova | oad | ncop | nced | op | ed | tv | \d{3,4} | (?:19|20)\d{2} |
    \d{2,3}-\d{2,3} | ep?\d{1,3}(?:-ep?\d{1,3})? |
    # 发布形容
    proper | repack | internal | limited | extended | uncut | unrated | remastered | complete |
    multi | dual | dubbed | subbed | multisub | hardsub | softsub | vision |
    chs | cht | eng | jpn | kor | jap | big5 | gb | 简体 | 繁体 | 动画 | 版 |
    (?:中|日|韩|英|粤|国|台|泰|法|德|俄|西|意)(?:语|配|文)(?:中字|双字|字幕|内嵌|外挂|配音)?
";

static NOISE_CORE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"(?ix)^(?:{NOISE_CORE})$")).expect("noise core token regex")
});

static NOISE_CORE_SUFFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"(?ix)(?:{NOISE_CORE})$")).expect("noise core suffix regex")
});

static NOISE_TOKEN_EXTRA_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?ix)^(?:{NOISE_TOKEN_EXTRA}|{NOISE_PLATFORMS})$"
    ))
    .expect("noise extra token regex")
});

/// 允许从标题**头部**剥离的 token：分类/画质词（`高清`、`电视剧`）与平台名。
/// 刻意不含英文发布形容词——`Complete Unknown`、`Limited` 这类词开头的真实片名
/// 不能被切头。
static NOISE_LEADING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"(?ix)^(?:{NOISE_CORE}|{NOISE_PLATFORMS})$"))
        .expect("noise leading token regex")
});

/// 季集/进度类尾缀允许含空格，需在整串上清除（`Mr. Robot Season 1`、`全 24 集`）。
static SPACED_TAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)(?:\s*(?:
            season\s*\d+(?:\s*[-~～到至]\s*\d+)? |
            \d+(?:st|nd|rd|th)\s*season |
            s\d{1,2}(?:\s*[-~～到至]\s*s?\d{1,2})? |
            第\s*[0-9一二三四五六七八九十百两]+\s*[季期部] |
            全\s*\d+\s*[集话話回] | 共\s*\d+\s*[集话話回] | 全\s*\d+\s*季 |
            dolby\s*(?:vision|atmos) |
            更新至.* | 更至.* | 更新中 | 连载 | 年番
        ))+$",
    )
    .expect("spaced tail regex")
});

/// 自带点号的技术词：在 `.` 被当作分隔符之前先粘合。
static DOTTED_TECH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)\b(?:
            (?P<codec>[hx])\.(?P<codec_num>26[45]) |
            (?P<audio>ddp?|dd\+|eac3|aac|ac3|dts(?:-hd)?(?:-ma)?|truehd|atmos|opus|flac)\s?(?P<ch_a>\d)[.\s](?P<ch_b>\d)
        )\b",
    )
    .expect("dotted tech regex")
});

/// 4 位年份候选；是否独立（前后不是数字/汉字）由 [`detect_year`] 手工判断，
/// 因为 regex crate 不支持环视，而消耗式边界会吞掉相邻候选之间的分隔符。
static YEAR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:19|20)\d{2}").expect("year regex"));

/// 季号：`S02`、`S01-S04`、`Season 2`、`2nd Season`、`第二季`、`第2期`、`第二部`。
static SEASON_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)
            (?:^|[^a-z0-9]) s(?P<s_start>\d{1,2}) (?:\s*[-~～到至]\s*s?(?P<s_end>\d{1,2}))? (?:[^0-9]|$) |
            (?:^|[^a-z]) season\s*(?P<w_start>\d{1,2}) (?:\s*[-~～到至]\s*(?P<w_end>\d{1,2}))? (?:[^0-9]|$) |
            (?:^|[^a-z0-9]) (?P<ord>\d{1,2})(?:st|nd|rd|th)\s*season |
            第\s*(?P<cn>[0-9一二三四五六七八九十两]+)\s*[季期部]
        ",
    )
    .expect("season regex")
});

/// 清洗结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedTitle {
    pub original: String,
    pub normalized: String,
    /// 标题里出现的发行年份（`(2024)`、`.2024.`），供元数据匹配缩小范围。
    /// 整个标题就是一个年份（`1917`）时不算。
    pub year: Option<i32>,
    /// 标题里出现的季号；区间（`S01-S04`）时为起始季。
    pub season: Option<i32>,
    /// 季区间的结束季（仅 `S01-S04`、`第1-3季` 这类写法）。
    pub season_end: Option<i32>,
}

/// 从分享标题中剥离噪声，得到更利于元数据匹配的剧名。
pub fn clean_media_title(title: &str) -> String {
    normalize_title_detailed(title).normalized
}

pub fn normalize_title_detailed(title: &str) -> NormalizedTitle {
    let original = title.trim().to_string();
    if original.is_empty() || original.to_ascii_lowercase().starts_with("http") {
        return NormalizedTitle {
            original: original.clone(),
            normalized: original,
            year: None,
            season: None,
            season_end: None,
        };
    }

    let prepared = pre_normalize(&original);
    let (season, season_end) = detect_season(&prepared);

    let mut cleaned = clean_pipeline(&prepared, BracketPolicy::Drop);
    if cleaned.is_empty() {
        // 括号里装的就是标题（`【庆余年】1080p`）：保留括号内容再来一遍。
        cleaned = clean_pipeline(&prepared, BracketPolicy::Keep);
    }

    let normalized = if cleaned.is_empty() {
        original.clone()
    } else {
        cleaned
    };
    let year = detect_year(&prepared).filter(|year| year.to_string() != normalized);

    NormalizedTitle {
        original,
        normalized,
        year,
        season,
        season_end,
    }
}

#[derive(Clone, Copy)]
enum BracketPolicy {
    /// 丢弃 `[]`、`【】`、`()`、`（）` 内的内容。
    Drop,
    /// 只去掉括号符号，保留内容。
    Keep,
}

fn clean_pipeline(prepared: &str, policy: BracketPolicy) -> String {
    let mut cleaned = strip_trailing_description(prepared);
    cleaned = strip_brackets_and_separators(&cleaned, policy);
    cleaned = collapse_spaces(&cleaned);
    cleaned = strip_leading_decorative_symbols(&cleaned);
    cleaned = prefer_primary_title_segment(&cleaned);
    cleaned = strip_spaced_tail(&cleaned);
    cleaned = strip_noise_tokens(&cleaned);
    cleaned = collapse_spaces(&cleaned);
    strip_leading_decorative_symbols(&cleaned)
}

/// 丢弃中文逗号 / 顿号之后的内容：`交锋 4K 完结，王凯` 里逗号后面是演员或
/// 描述，从不是标题的一部分。
///
/// ASCII 逗号只在其后含东亚文字时才当作这种分隔——`Love, Death & Robots`、
/// `I, Robot` 这类英文标题的逗号必须保留。首段剥空时不切（交给上层回退）。
fn strip_trailing_description(value: &str) -> String {
    let cut = value.char_indices().find_map(|(index, ch)| match ch {
        '，' | '、' => Some(index),
        ',' if value[index + 1..].chars().any(has_east_asian_char) => Some(index),
        _ => None,
    });
    match cut {
        Some(index) if !value[..index].trim().is_empty() => value[..index].to_string(),
        Some(index) => {
            // 逗号在最前面（`，庆余年`）：跳过它继续处理后面的内容。
            let width = value[index..].chars().next().map_or(0, char::len_utf8);
            strip_trailing_description(&value[index + width..])
        }
        None => value.to_string(),
    }
}

fn has_east_asian_char(ch: char) -> bool {
    is_cjk(ch) || is_kana(ch) || is_hangul(ch)
}

/// 全角转半角、ideographic space 归一化，并粘合自带点号的技术词。
fn pre_normalize(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for ch in value.chars() {
        let mapped = match ch {
            '\u{3000}' => ' ',
            'Ａ'..='Ｚ' => char::from_u32(ch as u32 - 'Ａ' as u32 + 'A' as u32).unwrap_or(ch),
            'ａ'..='ｚ' => char::from_u32(ch as u32 - 'ａ' as u32 + 'a' as u32).unwrap_or(ch),
            '０'..='９' => char::from_u32(ch as u32 - '０' as u32 + '0' as u32).unwrap_or(ch),
            '．' => '.',
            _ => ch,
        };
        output.push(mapped);
    }
    DOTTED_TECH_RE
        .replace_all(&output, |caps: &regex::Captures| {
            if let (Some(codec), Some(num)) = (caps.name("codec"), caps.name("codec_num")) {
                format!("{}{}", codec.as_str(), num.as_str())
            } else {
                format!(
                    "{}{}{}",
                    caps.name("audio").map(|m| m.as_str()).unwrap_or_default(),
                    caps.name("ch_a").map(|m| m.as_str()).unwrap_or_default(),
                    caps.name("ch_b").map(|m| m.as_str()).unwrap_or_default()
                )
            }
        })
        .into_owned()
}

fn detect_year(value: &str) -> Option<i32> {
    // 取最后一个候选：`2001太空漫游 (2024 修复)` 里靠后的更可能是发行年份标记。
    // 候选前后不能是数字（`12024`）或汉字/假名（`请回答1988` 的年份是剧名的一部分）。
    let chars: Vec<char> = value.chars().collect();
    YEAR_RE
        .find_iter(value)
        .filter_map(|matched| {
            let start_char = value[..matched.start()].chars().count();
            let end_char = start_char + 4;
            let before = start_char.checked_sub(1).and_then(|index| chars.get(index));
            let after = chars.get(end_char);
            let glued = before.is_some_and(|ch| ch.is_ascii_digit() || is_cjk(*ch) || is_kana(*ch))
                || after.is_some_and(|ch| ch.is_ascii_digit());
            if glued {
                return None;
            }
            matched.as_str().parse::<i32>().ok()
        })
        .last()
}

fn detect_season(value: &str) -> (Option<i32>, Option<i32>) {
    let Some(caps) = SEASON_RE.captures(value) else {
        return (None, None);
    };
    let parse = |name: &str| -> Option<i32> {
        let text = caps.name(name)?.as_str();
        text.parse::<i32>()
            .ok()
            .or_else(|| parse_chinese_number(text))
    };
    let start = parse("s_start")
        .or_else(|| parse("w_start"))
        .or_else(|| parse("ord"))
        .or_else(|| parse("cn"))
        .filter(|season| (1..=99).contains(season));
    let end = parse("s_end")
        .or_else(|| parse("w_end"))
        .filter(|end| start.is_some_and(|start| *end > start && *end <= 99));
    (start, end)
}

/// 解析 1–99 的中文数字（`二`、`十二`、`二十`、`两`）。
fn parse_chinese_number(text: &str) -> Option<i32> {
    let digit = |ch: char| -> Option<i32> {
        Some(match ch {
            '零' => 0,
            '一' => 1,
            '二' | '两' => 2,
            '三' => 3,
            '四' => 4,
            '五' => 5,
            '六' => 6,
            '七' => 7,
            '八' => 8,
            '九' => 9,
            _ => return None,
        })
    };
    let chars: Vec<char> = text.chars().collect();
    match chars.as_slice() {
        ['十'] => Some(10),
        [single] => digit(*single),
        ['十', ones] => Some(10 + digit(*ones)?),
        [tens, '十'] => Some(digit(*tens)? * 10),
        [tens, '十', ones] => Some(digit(*tens)? * 10 + digit(*ones)?),
        _ => None,
    }
}

/// 处理括号，并把 `.` `_` 与「非字母数字之间」的 `-` 归一化为空格。
///
/// `-` 在 ASCII 字母数字之间时保留，这样 `WEB-DL`、`Spider-Man`、`S01-S04`
/// 不会被拆成两个 token 而失去词表匹配能力。
fn strip_brackets_and_separators(value: &str, policy: BracketPolicy) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut output = String::with_capacity(value.len());
    let mut bracket_depth = 0usize;

    for (index, &ch) in chars.iter().enumerate() {
        match ch {
            '[' | '【' | '(' | '（' => {
                bracket_depth += 1;
                if matches!(policy, BracketPolicy::Keep) {
                    output.push(' ');
                }
            }
            ']' | '】' | ')' | '）' => {
                bracket_depth = bracket_depth.saturating_sub(1);
                if matches!(policy, BracketPolicy::Keep) {
                    output.push(' ');
                }
            }
            // 书名号 / 日文引号里装的是标题本身：只去符号、留内容
            '《' | '》' | '「' | '」' | '『' | '』' => output.push(' '),
            _ if bracket_depth > 0 && matches!(policy, BracketPolicy::Drop) => {}
            '.' | '_' => output.push(' '),
            '-' => {
                let previous = index.checked_sub(1).and_then(|i| chars.get(i)).copied();
                let next = chars.get(index + 1).copied();
                let hyphenated = previous.is_some_and(|c| c.is_ascii_alphanumeric())
                    && next.is_some_and(|c| c.is_ascii_alphanumeric());
                output.push(if hyphenated { '-' } else { ' ' });
            }
            _ => output.push(ch),
        }
    }

    output
}

/// 整串清除含空格的季集/进度尾缀，循环到稳定以处理交错情况。
fn strip_spaced_tail(value: &str) -> String {
    let mut current = value.trim().to_string();
    for _ in 0..4 {
        let next = SPACED_TAIL_RE.replace(&current, "").to_string();
        let next = next.trim_end().to_string();
        if next == current {
            break;
        }
        current = next;
    }
    current
}

/// 从尾部（以及头部）逐个 token 剥离噪声。
///
/// 尾部遇到第一个非噪声 token 即停止；token 内部的尾缀（`4K修复版`、`1080p`）
/// 也会被继续剥离，直到该 token 干净或为空。
fn strip_noise_tokens(value: &str) -> String {
    let mut tokens: Vec<String> = value.split_whitespace().map(str::to_string).collect();

    while tokens.len() > 1 {
        let last = tokens.last().expect("checked non-empty").clone();
        match classify_token(&last) {
            TokenNoise::Whole => {
                tokens.pop();
            }
            TokenNoise::Suffix(remaining) => {
                tokens.pop();
                if !remaining.is_empty() {
                    tokens.push(remaining);
                }
            }
            TokenNoise::Clean => break,
        }
    }

    // 标题前的分类词/画质词同样应清除（`高清 庆余年`、`电视剧 三体`、`Netflix 鱿鱼游戏`）。
    while tokens.len() > 1 {
        let first = tokens.first().expect("checked non-empty").clone();
        if NOISE_LEADING_RE.is_match(&first) {
            tokens.remove(0);
        } else {
            break;
        }
    }

    // 只剩一个 token 且它本身就是噪声（`【庆余年】1080p` 丢掉括号后只剩 `1080p`）：
    // 返回空，让上层回退到「保留括号内容」再清洗；上层最终仍会兜底回原值。
    if tokens.len() == 1 && matches!(classify_token(&tokens[0]), TokenNoise::Whole) {
        return String::new();
    }

    tokens.join(" ")
}

enum TokenNoise {
    /// 整个 token 都是噪声。
    Whole,
    /// 仅尾部是噪声，携带剥离后的剩余内容（可能为空）。
    Suffix(String),
    /// 不是噪声。
    Clean,
}

fn classify_token(token: &str) -> TokenNoise {
    if NOISE_CORE_RE.is_match(token) || NOISE_TOKEN_EXTRA_RE.is_match(token) {
        return TokenNoise::Whole;
    }
    // 连字符组合（`x265-FLUX`、`264-GROUP`、`HEVC-10bit`）：任一半是完整噪声词，
    // 整个 token 就是发布标记。`Spider-Man` 两半都不是噪声，不受影响。
    if let Some((left, right)) = token.split_once('-') {
        let is_noise = |part: &str| {
            !part.is_empty()
                && (NOISE_CORE_RE.is_match(part) || NOISE_TOKEN_EXTRA_RE.is_match(part))
        };
        if is_noise(left) || is_noise(right) {
            return TokenNoise::Whole;
        }
    }
    if let Some(found) = NOISE_CORE_SUFFIX_RE.find(token) {
        if found.start() > 0 {
            // 剥掉噪声尾缀后只剩数字与符号（`0+HiFi` → `0+`、`2.0` 之类）：
            // 这不是剧名残片，而是同一个技术标记的另一半。
            let remaining = token[..found.start()]
                .trim_end_matches(|ch: char| !ch.is_alphanumeric())
                .to_string();
            if remaining.chars().all(|ch| ch.is_ascii_digit()) {
                return TokenNoise::Whole;
            }
            return TokenNoise::Suffix(remaining);
        }
    }
    TokenNoise::Clean
}

fn collapse_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 去掉标题前方的 emoji / 符号装饰（如 🗄 📺 ★），避免干扰元数据匹配。
fn strip_leading_decorative_symbols(value: &str) -> String {
    let trimmed = value.trim_start();
    let mut chars = trimmed.chars().peekable();
    while let Some(&ch) = chars.peek() {
        if is_decorative_leading_char(ch) {
            chars.next();
            continue;
        }
        break;
    }
    chars.collect::<String>().trim_start().to_string()
}

fn is_decorative_leading_char(ch: char) -> bool {
    if ch.is_whitespace() {
        return true;
    }
    // 保留中日韩文字、假名、字母数字与常见连接符
    if ch.is_ascii_alphanumeric() {
        return false;
    }
    if is_cjk(ch) || is_kana(ch) || ('\u{ac00}'..='\u{d7af}').contains(&ch) {
        return false;
    }
    if ('\u{00c0}'..='\u{024f}').contains(&ch) {
        return false;
    }
    if matches!(
        ch,
        '·' | '・' | '•' | '\'' | '’' | '′' | '″' | '"' | '“' | '”' | '!' | '！' | '?' | '？'
    ) {
        return false;
    }
    // So/Sm/Sk 等装饰符号与 emoji 区间
    matches!(
        ch,
        '\u{2000}'..='\u{206f}'
            | '\u{2190}'..='\u{21ff}'
            | '\u{2300}'..='\u{23ff}'
            | '\u{2460}'..='\u{24ff}'
            | '\u{2500}'..='\u{27bf}'
            | '\u{2900}'..='\u{297f}'
            | '\u{2b00}'..='\u{2bff}'
            | '\u{3000}'..='\u{303f}'
            | '\u{fe00}'..='\u{fe0f}'
            | '\u{1f000}'..='\u{1faff}'
    ) || matches!(
        ch,
        '★' | '☆'
            | '✦'
            | '✧'
            | '✪'
            | '✩'
            | '❖'
            | '※'
            | '◆'
            | '◇'
            | '■'
            | '□'
            | '●'
            | '○'
            | '◎'
            | '◉'
            | '♦'
            | '♠'
            | '♣'
            | '♥'
            | '▶'
            | '▷'
            | '◀'
            | '◁'
            | '►'
            | '◄'
            | '▲'
            | '△'
            | '▼'
            | '▽'
            | '✓'
            | '✔'
            | '✕'
            | '✖'
            | '✗'
            | '✘'
            | '＋'
            | '－'
            | '＝'
            | '｜'
            | '¦'
            | '§'
            | '¶'
            | '†'
            | '‡'
            | '‣'
            | '⁃'
            | '⁎'
            | '⁑'
            | '⁓'
            | '⁕'
            | '#'
            | '@'
            | '~'
            | '`'
            | '^'
            | '*'
            | '='
            | '+'
            | '|'
            | '\\'
            | '/'
            | '<'
            | '>'
            | '{'
            | '}'
            | '['
            | ']'
    )
}

fn is_cjk(ch: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&ch) || ('\u{3400}'..='\u{4dbf}').contains(&ch)
}

fn is_kana(ch: char) -> bool {
    ('\u{3040}'..='\u{30ff}').contains(&ch)
}

fn is_hangul(ch: char) -> bool {
    ('\u{ac00}'..='\u{d7af}').contains(&ch)
}

/// 段落是否含东亚文字（汉字 / 假名 / 谚文）。
fn has_east_asian(segment: &str) -> bool {
    segment.chars().any(has_east_asian_char)
}

/// 段落是否是「另一种语言」：含拉丁字母、不含东亚文字，且不是罗马数字。
///
/// 罗马数字（`II`、`III`）是续集编号而不是外文标题，`三体 II` 必须保留。
fn is_latin_segment(segment: &str) -> bool {
    if has_east_asian(segment) || !segment.chars().any(|ch| ch.is_ascii_alphabetic()) {
        return false;
    }
    !segment
        .chars()
        .all(|ch| matches!(ch.to_ascii_uppercase(), 'I' | 'V' | 'X'))
}

/// 中日/中英并列标题时挑出更适合中文元数据匹配的一段。
///
/// 两条规则，都只在**存在明确的多段并列**时生效：
///
/// 1. 显式分隔符（`/`、`|`、`／`）：这类写法几乎总是「中文名 / 原名」。优先取
///    含汉字且不含假名的那段（中文），其次任一含东亚文字的段，都没有时取第一段
///    （`Breaking Bad / Netflix` → `Breaking Bad`）。
/// 2. 空白分隔：若同时存在东亚文字段与拉丁字母段，只保留东亚文字段（保持顺序）。
///    `咒术回战 Jujutsu Kaisen 第2季` → `咒术回战 第2季`（尾缀交给下一步）。
///
/// 关键点：**不再**因为标题内部出现假名就截断。`鬼滅の刃`、`四月は君の嘘`
/// 是单段标题，必须原样保留；旧实现在第一个假名处切断，把 `鬼滅の刃` 变成
/// `鬼滅`、`君の名は。` 变成 `君`，这正是日文剧名匹配失败的主因。
///
/// 纯数字段与罗马数字不算「另一种语言」，否则 `沙丘 2`、`三体 II` 会丢掉续集号。
fn prefer_primary_title_segment(value: &str) -> String {
    let title = value.trim();
    if title.is_empty() {
        return String::new();
    }

    let parts: Vec<&str> = title
        .split(['|', '/', '／'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() > 1 {
        let chinese = parts
            .iter()
            .find(|part| part.chars().any(is_cjk) && !part.chars().any(is_kana));
        let east_asian = parts.iter().find(|part| has_east_asian(part));
        if let Some(primary) = chinese.or(east_asian).or(parts.first()) {
            return primary.to_string();
        }
    }

    let segments: Vec<&str> = title.split_whitespace().collect();
    if segments.len() > 1 {
        let has_east_asian_segment = segments.iter().any(|segment| has_east_asian(segment));
        let has_latin_segment = segments.iter().any(|segment| is_latin_segment(segment));
        if has_east_asian_segment && has_latin_segment {
            return segments
                .iter()
                .filter(|segment| !is_latin_segment(segment))
                .copied()
                .collect::<Vec<_>>()
                .join(" ");
        }
    }

    title.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_fansub_and_quality_noise() {
        assert_eq!(
            clean_media_title("【字幕组】庆余年 1080p S01-S04 全集"),
            "庆余年"
        );
        assert_eq!(clean_media_title("庆余年（2024）[简中]"), "庆余年");
        assert_eq!(
            clean_media_title("孤独摇滚！ / Bocchi the Rock!"),
            "孤独摇滚！"
        );
        assert_eq!(clean_media_title("凡人修仙传 4K 高码率"), "凡人修仙传");
        assert_eq!(clean_media_title("凡人修仙传 高清 国粤双语"), "凡人修仙传");
    }

    #[test]
    fn strips_leading_emoji_and_symbols() {
        assert_eq!(clean_media_title("🗄 庆余年"), "庆余年");
        assert_eq!(clean_media_title("📺庆余年 1080p"), "庆余年");
        assert_eq!(clean_media_title("★ 孤独摇滚！"), "孤独摇滚！");
        assert_eq!(clean_media_title("🗄【字幕组】庆余年 S01"), "庆余年");
    }

    #[test]
    fn keeps_urls_untouched() {
        let url = "https://pan.quark.cn/s/abc";
        assert_eq!(clean_media_title(url), url);
    }

    /// 日文标题内部的假名不是分隔符。旧实现在第一个假名处截断，
    /// 把 `鬼滅の刃` 变成 `鬼滅`，是日文剧名匹配失败的主因。
    #[test]
    fn keeps_japanese_titles_with_inner_kana_intact() {
        assert_eq!(clean_media_title("鬼滅の刃"), "鬼滅の刃");
        assert_eq!(clean_media_title("進撃の巨人"), "進撃の巨人");
        assert_eq!(clean_media_title("君の名は。"), "君の名は。");
        assert_eq!(clean_media_title("葬送のフリーレン"), "葬送のフリーレン");
        assert_eq!(clean_media_title("四月は君の嘘"), "四月は君の嘘");
        assert_eq!(clean_media_title("呪術廻戦 第2期"), "呪術廻戦");
        assert!(
            clean_media_title("Re:ゼロから始める異世界生活").starts_with("Re:ゼロ"),
            "含冒号的日文标题不应被截断"
        );
    }

    #[test]
    fn picks_chinese_segment_from_bilingual_titles() {
        assert_eq!(clean_media_title("鱿鱼游戏 Squid Game"), "鱿鱼游戏");
        assert_eq!(clean_media_title("Stranger Things 怪奇物语"), "怪奇物语");
        assert_eq!(clean_media_title("The Last of Us 最后生还者"), "最后生还者");
        assert_eq!(clean_media_title("Spy×Family 间谍过家家"), "间谍过家家");
        // 两端都是中文时不做取舍，交给噪声清洗
        assert_eq!(
            clean_media_title("机动战士高达 闪光的哈萨维"),
            "机动战士高达 闪光的哈萨维"
        );
    }

    /// 日文原名 + 罗马音 / 中文名并列：优先中文，其次日文，绝不取罗马音。
    #[test]
    fn bilingual_japanese_titles_prefer_chinese_then_japanese() {
        assert_eq!(
            clean_media_title("葬送のフリーレン / Sousou no Frieren"),
            "葬送のフリーレン"
        );
        assert_eq!(
            clean_media_title("葬送のフリーレン / 葬送的芙莉莲"),
            "葬送的芙莉莲"
        );
        assert_eq!(
            clean_media_title("Sousou no Frieren 葬送のフリーレン"),
            "葬送のフリーレン"
        );
        assert_eq!(
            clean_media_title("咒术回战 Jujutsu Kaisen 第2季"),
            "咒术回战"
        );
        // 没有东亚文字时取第一段，而不是整串
        assert_eq!(clean_media_title("Breaking Bad / Netflix"), "Breaking Bad");
    }

    /// 纯数字段与罗马数字不算「另一种语言」，否则续集编号会被丢掉。
    #[test]
    fn keeps_sequel_numbers() {
        assert_eq!(clean_media_title("沙丘 2"), "沙丘 2");
        assert_eq!(clean_media_title("沙丘2 Dune Part Two"), "沙丘2");
        assert_eq!(clean_media_title("Fast & Furious 9"), "Fast & Furious 9");
        assert_eq!(clean_media_title("请回答1988"), "请回答1988");
        assert_eq!(clean_media_title("1917"), "1917");
        assert_eq!(clean_media_title("三体 II"), "三体 II");
    }

    /// 一个未登记的噪声词不应阻断它前面的噪声被清除。
    #[test]
    fn noise_after_unknown_token_is_still_stripped() {
        assert_eq!(clean_media_title("信号 1080p 韩语中字"), "信号");
        assert_eq!(
            clean_media_title("【字幕组】[简繁外挂] 葬送的芙莉莲 第1季 全28话"),
            "葬送的芙莉莲"
        );
        assert_eq!(
            clean_media_title("庆余年.WEB-DL.2160p.HDR.国语中字"),
            "庆余年"
        );
        assert_eq!(
            clean_media_title("庆余年_2024_4K_高码率_更新至30集"),
            "庆余年"
        );
    }

    #[test]
    fn strips_bare_years_and_category_words() {
        assert_eq!(clean_media_title("庆余年 2024"), "庆余年");
        assert_eq!(clean_media_title("漫长的季节 2023"), "漫长的季节");
        assert_eq!(clean_media_title("沉默的真相 2020 高清"), "沉默的真相");
        assert_eq!(clean_media_title("三体 电视剧"), "三体");
        assert_eq!(clean_media_title("中国奇谭 2023 全8集"), "中国奇谭");
        assert_eq!(clean_media_title("遮天 3D 国漫 4K"), "遮天");
        assert_eq!(clean_media_title("斗罗大陆 年番 更新中"), "斗罗大陆");
        assert_eq!(clean_media_title("甄嬛传 4K修复版"), "甄嬛传");
        assert_eq!(clean_media_title("名侦探柯南 剧场版"), "名侦探柯南");
        assert_eq!(clean_media_title("Mr. Robot Season 1"), "Mr Robot");
        assert_eq!(
            clean_media_title("Game of Thrones S01-S08"),
            "Game of Thrones"
        );
    }

    /// 场景发布命名：点号分隔 + 音频声道 + 编码 + 发布组。
    ///
    /// `DDP5.1` 若被点号拆成 `5` 与 `1`，尾部的 `1` 不是噪声，会阻断整段清洗。
    #[test]
    fn handles_scene_release_names() {
        assert_eq!(
            clean_media_title("Joy.of.Life.S02.2024.1080p.WEB-DL.DDP5.1.H.264-FLUX"),
            "Joy of Life"
        );
        assert_eq!(
            clean_media_title("庆余年.2024.2160p.WEB-DL.H265.AAC2.0-GROUP"),
            "庆余年"
        );
        assert_eq!(
            clean_media_title("Frieren.Beyond.Journeys.End.S01E01.1080p.NF.WEB-DL.x265-HONE"),
            "Frieren Beyond Journeys End"
        );
        assert_eq!(
            clean_media_title("Dune Part Two 2024 IMAX 2160p DV HDR10+ Atmos TrueHD 7.1"),
            "Dune Part Two"
        );
        // 连字符组合：一半是噪声即整个 token 是噪声；`Spider-Man` 不受影响
        assert_eq!(clean_media_title("Arcane S02 HEVC-10bit"), "Arcane");
        assert_eq!(
            clean_media_title("Spider-Man: No Way Home"),
            "Spider-Man: No Way Home"
        );
    }

    /// 书名号里的是标题本身；普通括号剥空后要回退保留括号内容。
    #[test]
    fn keeps_title_inside_title_marks_and_falls_back_for_bracketed_titles() {
        assert_eq!(clean_media_title("《庆余年》第二季"), "庆余年");
        assert_eq!(clean_media_title("《庆余年》 4K 全36集"), "庆余年");
        assert_eq!(
            clean_media_title("「葬送のフリーレン」第1話"),
            "葬送のフリーレン"
        );
        assert_eq!(clean_media_title("【庆余年】1080p"), "庆余年");
        assert_eq!(
            clean_media_title("[Sakurato] 葬送的芙莉莲 [01-28][1080p]"),
            "葬送的芙莉莲"
        );
        assert_eq!(clean_media_title("(庆余年)"), "庆余年");
    }

    /// 全角字母数字、平台名、发布形容词与集数区间。
    #[test]
    fn strips_platforms_release_adjectives_and_episode_ranges() {
        assert_eq!(clean_media_title("庆余年 １０８０Ｐ ＷＥＢ"), "庆余年");
        assert_eq!(clean_media_title("Netflix 鱿鱼游戏 S02"), "鱿鱼游戏");
        assert_eq!(clean_media_title("鱿鱼游戏 NF WEB-DL 1080p"), "鱿鱼游戏");
        assert_eq!(clean_media_title("繁花 全30集 4K 豆瓣8.7分"), "繁花");
        assert_eq!(clean_media_title("繁花 01-30 国语中字"), "繁花");
        assert_eq!(clean_media_title("繁花 EP01-30"), "繁花");
        assert_eq!(clean_media_title("繁花 E01 1080p"), "繁花");
        assert_eq!(
            clean_media_title("The Bear S03 COMPLETE REPACK"),
            "The Bear"
        );
        assert_eq!(
            clean_media_title("鬼灭之刃 TV动画 日语中字 生肉"),
            "鬼灭之刃"
        );
        assert_eq!(clean_media_title("Dune 2021 Dolby Vision"), "Dune");
    }

    /// 用户实测样本：中文逗号后的演员名、被空格拆开的 `DDP2 0`、`0+HiFi` 残片。
    #[test]
    fn strips_actor_suffix_and_split_audio_channel_markers() {
        assert_eq!(
            clean_media_title(
                "交锋 4K HDR SDR DV杜比视界 高码率 DDP2 0+HiFi 中字全40集 完结，王凯"
            ),
            "交锋"
        );
        assert_eq!(clean_media_title("交锋 4K DDP2.0 中字，王凯 张译"), "交锋");
        assert_eq!(clean_media_title("繁花，胡歌、马伊琍"), "繁花");
        // 英文标题里的逗号是标题的一部分
        assert_eq!(
            clean_media_title("Love, Death & Robots"),
            "Love, Death & Robots"
        );
        assert_eq!(clean_media_title("I, Robot 2004 1080p"), "I, Robot");
        // 逗号前为空时不切，回退到整体清洗
        assert_eq!(clean_media_title("，庆余年"), "庆余年");
    }

    /// 年份与季号作为提示返回，不再只是被丢掉。
    #[test]
    fn extracts_year_and_season_hints() {
        let detail = normalize_title_detailed("庆余年 (2024) 第二季 4K");
        assert_eq!(detail.normalized, "庆余年");
        assert_eq!(detail.year, Some(2024));
        assert_eq!(detail.season, Some(2));
        assert_eq!(detail.season_end, None);

        let detail = normalize_title_detailed("Game.of.Thrones.S01-S08.2011.1080p");
        assert_eq!(detail.normalized, "Game of Thrones");
        assert_eq!(detail.year, Some(2011));
        assert_eq!(detail.season, Some(1));
        assert_eq!(detail.season_end, Some(8));

        let detail = normalize_title_detailed("咒术回战 Season 2 [2023]");
        assert_eq!(detail.season, Some(2));
        assert_eq!(detail.year, Some(2023));

        let detail = normalize_title_detailed("呪術廻戦 第十二期");
        assert_eq!(detail.season, Some(12));

        let detail = normalize_title_detailed("Frieren.S01E05.1080p");
        assert_eq!(detail.season, Some(1), "S01E05 只取季号，不受集号影响");

        // 整个标题就是年份时不算年份；与汉字粘连的年份是剧名的一部分
        assert_eq!(normalize_title_detailed("1917").year, None);
        assert_eq!(normalize_title_detailed("请回答1988").year, None);
        assert_eq!(normalize_title_detailed("请回答1988 2015").year, Some(2015));
        // 没有季标记时不臆造
        assert_eq!(normalize_title_detailed("庆余年 1080p").season, None);
        // `4K` 不是 S4；`2nd Season` 是 2
        assert_eq!(normalize_title_detailed("庆余年 4K").season, None);
        assert_eq!(
            normalize_title_detailed("Mob Psycho 100 2nd Season").season,
            Some(2)
        );
    }

    /// 短词只允许整 token 匹配：`Robots`、`Drama` 这类真实剧名不能被切尾。
    #[test]
    fn short_noise_tokens_never_match_as_suffix() {
        assert_eq!(clean_media_title("Robots"), "Robots");
        assert_eq!(clean_media_title("Drama"), "Drama");
        assert_eq!(
            clean_media_title("Pirates of the Caribbean"),
            "Pirates of the Caribbean"
        );
        assert_eq!(clean_media_title("The Matrix"), "The Matrix");
        // `国` / `版` / `动画` 只能整 token 清除，`三国`、`新版`、`动画人生` 不能切尾
        assert_eq!(clean_media_title("三国"), "三国");
        assert_eq!(clean_media_title("蜡笔小新 动画"), "蜡笔小新");
        assert_eq!(clean_media_title("动画人生"), "动画人生");
        assert_eq!(clean_media_title("Complete Unknown"), "Complete Unknown");
    }

    /// 连字符在 ASCII 字母数字之间保留，保证 `WEB-DL`、`Spider-Man` 完整。
    #[test]
    fn keeps_intra_word_hyphens() {
        assert_eq!(
            clean_media_title("Spider-Man: No Way Home"),
            "Spider-Man: No Way Home"
        );
        assert_eq!(clean_media_title("庆余年 WEB-DL 1080p"), "庆余年");
    }

    /// 清洗结果为空时必须回退到原值，不能返回空字符串。
    #[test]
    fn falls_back_to_original_when_everything_is_noise() {
        assert_eq!(clean_media_title("全12集"), "全12集");
        assert_eq!(clean_media_title("1080p"), "1080p");
    }

    #[test]
    fn parses_chinese_numbers_up_to_ninety_nine() {
        assert_eq!(parse_chinese_number("一"), Some(1));
        assert_eq!(parse_chinese_number("两"), Some(2));
        assert_eq!(parse_chinese_number("十"), Some(10));
        assert_eq!(parse_chinese_number("十二"), Some(12));
        assert_eq!(parse_chinese_number("二十"), Some(20));
        assert_eq!(parse_chinese_number("二十三"), Some(23));
        assert_eq!(parse_chinese_number("百"), None);
    }
}
