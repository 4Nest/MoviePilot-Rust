//! 从发布结构恢复别名；语言标签、季度和集号不能参与片名竞争。

use super::model::MetaResult;
use super::parser::{cn_number_to_i64, is_chinese_char};
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

static LABEL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"(?i)^(?:(?:简|簡|繁)(?:体|體|中|日|英)?|中日|中英|双语|雙語|",
        r"字幕|内封|內封|内嵌|內嵌|外挂|外掛|中文|日文|英文|日语|日語|粤语|粵語|",
        r"国语|國語|国英|國英|国配|國配|中字|音轨|音軌|CHS|CHT|GB|BIG5|JPSC|JPTC|JP|EN|UTF8|ASS|SRT|PGS|MP4|MKV|",
        r"\s|[/&+_／-])+$"
    ))
    .unwrap()
});
static CATEGORY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)^(?:\d{4}年)?\d{1,2}月(?:新番|番)?$|^(?:\d{2,4}年)?(?:新番|日漫|国漫|日剧|动漫|動畫|动画|TV版|转发|轉發)$|^(?:ViuTV)?(?:粤语|粵語|日配版)$",
    )
    .unwrap()
});
static CATEGORY_PREFIX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(?:\d{4}年)?\d{1,2}月(?:新番|番)\s*").unwrap());
static BOUNDARY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"(?i)\s+-\s+\d{1,4}(?:v\d+)?(?:\s|$)|",
        r"(?:^|\s)S\d{1,4}E\d{1,4}|第\s*\d{1,4}\s*[集话話]|",
        r"(?:^|\s)(?:8|10|12)[- ]?bit\b|",
        r"(?:^|\s)(?:BDRip|BDRemux|BluRay|WEB[- .]?DL|WEBRip)\b|",
        r"(?:^|\s)(?:\d{3,4}[pi]|\d{3,4}x\d{3,4})(?:\b|_)|",
        r"[\[(](?:\d{3,4}[pi]|\d{3,4}x\d{3,4})(?:\s|[\])])"
    ))
    .unwrap()
});
static EPISODE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
    r"(?i)^(?:(?:总|總)?第|EP?|TV\s*)?(\d{1,4})(?:v\d+)?(?:\s*[-~～–—]\s*(\d{1,4})(?:v\d+)?)?(?:[集话話]|\s*(?:END|FIN|(?:精修|精校|修正)?合集))?$"
).unwrap()
});
static SEASON: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"(?i)第\s*([0-9一二三四五六七八九十百零]+)\s*[季期]|",
        r"(?:\bSeason\s*|\bS)(\d{1,2})\b|\b(\d{1,2})(?:st|nd|rd|th)(?:\s+Season\b|$)"
    ))
    .unwrap()
});
static YEAR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?:\s|^)[(（]?((?:19|20)\d{2})[)）]?\s*$").unwrap());
static EXPLICIT_SEASON: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bS\d{1,4}E\d{1,4}").unwrap());
static MIXED_ALIAS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(.+[\p{Han}])\s+([A-Za-z][A-Za-z\s'-]+)$").unwrap());
static TECH: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"(?i)\b(?:\d{3,4}[pi]|\d{3,4}x\d{3,4}|WEB[- .]?DL|WEBRip|BluRay|BDRip|",
        r"HEVC|AVC|AV1|[hx][ .]?26[45]|AAC|FLAC|[0-9a-f]{8})\b|",
        r"(?i)(?:^|[ _])(?:Ma|Hi)?(?:8|10|12)[- ]?(?:bit|p)(?:_|\b)"
    ))
    .unwrap()
});

/// 整段匹配发布标签，避免把含 Movie、Animation 等单词的真实片名过滤掉。
pub(super) fn is_label(value: &str) -> bool {
    LABEL.is_match(value.trim()) || CATEGORY.is_match(value.trim())
}

static BONUS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^(?:NCOP|NCED|CM|PV|Menu)\d*(?:_\d+)?$").unwrap());
static ENCODE_BLOCK: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^(?:Ma|Hi)(?:8|10|12)p_\d{3,4}p$").unwrap());
static BATCH_SEASON: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\bS(\d{1,4})(?:\s*[-~–]\s*S?(\d{1,4}))?\b").unwrap());
static MOVIE_BATCH: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)[\[【]MOVIE(?:\s|[\]】])").unwrap());
static BARE_BIT_DEPTH: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^(?:8|10|12)[- ]?bit\b").unwrap());
static RESOLUTION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:^|[\s\[【(])((?:\d{3,4}x)?\d{3,4}[pi]?)(?:$|[\s\]】)])").unwrap()
});
static EXPLICIT_EPISODE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"(?i)\bS(\d{1,4})E(\d{1,4})",
        r"(?:\s*[-~–]\s*(?:S(\d{1,4}))?E?(\d{1,4}))?"
    ))
    .unwrap()
});

/// 显式季集对照以第一组为准；尾部括号里的正篇对应集不能扩大特典范围。
pub(super) fn restore_explicit_episode(meta: &mut MetaResult, title: &str) {
    let Some(cap) = EXPLICIT_EPISODE.captures(title) else {
        return;
    };
    let season = cap[1].parse::<i64>().unwrap_or_default();
    let episode = cap[2].parse::<i64>().unwrap_or_default();
    let end_season = cap.get(3).and_then(|m| m.as_str().parse::<i64>().ok());
    let end_episode = cap.get(4).and_then(|m| m.as_str().parse::<i64>().ok());
    // 跨季范围交给原有逻辑处理，不能用两端集号计算整包集数。
    if end_season.is_some_and(|end| end != season) {
        return;
    }
    // 连写 E01E02、逗号列表等不是主编号加注释，也交给原有逻辑处理。
    let tail = &title[cap.get(0).unwrap().end()..];
    if tail.starts_with(['E', 'e', ',']) {
        return;
    }
    meta.begin_season = Some(season);
    meta.end_season = None;
    meta.total_season = 1;
    meta.begin_episode = Some(episode);
    meta.end_episode = end_episode.filter(|end| *end > episode);
    meta.total_episode = meta.end_episode.map_or(1, |end| end - episode + 1);
    meta.media_type = "电视剧".to_string();
}

/// 同时识别半角和全角发布括号，保留片名内部原有的标点。
fn bracket_block(value: &str) -> Option<(&str, &str)> {
    let opening = value.chars().next()?;
    let closing = match opening {
        '[' => ']',
        '【' => '】',
        _ => return None,
    };
    let end = value.find(closing)?;
    Some((
        &value[opening.len_utf8()..end],
        &value[end + closing.len_utf8()..],
    ))
}

/// 前置字幕标签不能参与普通英文片名分词；原文仍由调用方保存。
pub(super) fn strip_leading_labels(title: &str) -> Cow<'_, str> {
    let Some((_, mut rest)) = bracket_block(title) else {
        return Cow::Borrowed(title);
    };
    let group_end = title.len() - rest.len();
    let mut removed = false;
    while let Some((content, tail)) = bracket_block(rest.trim_start()) {
        if !is_label(content) {
            break;
        }
        rest = tail;
        removed = true;
    }
    if removed {
        Cow::Owned(format!("{} {}", &title[..group_end], rest.trim_start()))
    } else {
        Cow::Borrowed(title)
    }
}

/// 连续正文中的强调括号属于片名，已知发布标签和季集字段仍作为边界。
fn name_chunk_end(value: &str) -> usize {
    for (index, ch) in value.char_indices() {
        if !matches!(ch, '[' | '【') {
            continue;
        }
        let Some((content, tail)) = bracket_block(&value[index..]) else {
            return index;
        };
        let content = content.trim();
        let continuation = tail.trim_start();
        if index == 0
            || is_label(content)
            || EPISODE.is_match(content)
            || TECH.is_match(content)
            || BONUS.is_match(content)
            || SEASON.is_match(content)
            || EXPLICIT_SEASON.is_match(content)
            || continuation.is_empty()
            || continuation.starts_with(['[', '【', '/', '／'])
        {
            return index;
        }
    }
    value.len()
}

/// 处理有发布组和季集或技术边界的 BT 名称，不介入普通点分 PT 电影名。
pub(super) fn restore_names(meta: &mut MetaResult, title: &str) -> bool {
    let title = title.trim();
    if !title.starts_with(['[', '【']) {
        return false;
    }
    let Some((group, tail)) = bracket_block(title) else {
        return false;
    };
    let mut rest = tail.trim();
    // [片名][01] 只有一个名字，首括号不能被无条件当作字幕组剥离。
    if bracket_block(rest).is_some_and(|(content, _)| EPISODE.is_match(content)) {
        return false;
    }
    let mut names = Vec::new();
    let mut found_boundary = false;
    let mut explicit_episode_boundary = false;
    let mut bare_tech_boundary = false;
    let mut non_episode_file = false;
    let mut bit_depth_boundary = false;
    while !rest.is_empty() {
        if let Some((content, tail)) = bracket_block(rest) {
            let content = content.trim().trim_start_matches(['[', '【']);
            if let Some(cap) = EPISODE.captures(content) {
                let begin = cap[1].parse::<i64>().unwrap_or_default();
                let end = cap.get(2).and_then(|v| v.as_str().parse::<i64>().ok());
                if (1900..=2099).contains(&begin) && end.is_none() {
                    meta.year = Some(begin.to_string());
                    rest = tail.trim();
                    continue;
                }
                // [1987] 等年份不能覆盖集号；显式 SxxEyy 仍由主解析器处理。
                if !(1900..=2099).contains(&begin) && end.is_none_or(|end| end >= begin) {
                    meta.begin_episode = Some(begin);
                    meta.end_episode = end;
                    meta.total_episode = end.map_or(1, |end| end - begin + 1);
                    meta.media_type = "电视剧".to_string();
                }
                found_boundary = true;
                explicit_episode_boundary = true;
                break;
            }
            // PV/NCOP 等编号描述预告或片头，不是正篇集号。
            if meta.isfile && BONUS.is_match(content) {
                found_boundary = true;
                non_episode_file = true;
                break;
            }
            if TECH.is_match(content) {
                non_episode_file = meta.isfile && ENCODE_BLOCK.is_match(content);
                found_boundary = true;
                explicit_episode_boundary = meta.isfile
                    && content.len() == 8
                    && content.chars().all(|ch| ch.is_ascii_hexdigit());
                break;
            }
            if !is_label(content) {
                names.push(content);
            }
            rest = tail.trim();
        } else {
            let end = name_chunk_end(rest);
            if end == 0 {
                break;
            }
            let chunk = rest[..end].trim().trim_matches('★');
            let chunk = CATEGORY_PREFIX
                .find(chunk)
                .map_or(chunk, |label| &chunk[label.end()..]);
            if let Some(boundary) = BOUNDARY.find(chunk) {
                names.push(chunk[..boundary.start()].trim());
                explicit_episode_boundary =
                    boundary.as_str().trim().starts_with(['-', 'S', 's', '第']);
                bare_tech_boundary = !explicit_episode_boundary;
                bit_depth_boundary = BARE_BIT_DEPTH.is_match(boundary.as_str().trim());
                found_boundary = true;
                break;
            }
            if !chunk.is_empty() && !is_label(chunk) {
                names.push(chunk);
            }
            rest = &rest[end..];
        }
    }
    if !found_boundary || names.is_empty() {
        return false;
    }
    // 无显式季集、特典或位深边界的普通英文名沿用 anitomy，保留其集名判定。
    if !explicit_episode_boundary
        && !non_episode_file
        && !bit_depth_boundary
        && !names
            .iter()
            .any(|s| s.contains('/') || s.chars().any(is_chinese_char))
    {
        return false;
    }
    let mut chinese = None;
    let mut latin = None;
    let mut season = None;
    for name in names
        .iter()
        .flat_map(|name| name.split('/'))
        .flat_map(|name| {
            name.split(if name.chars().any(is_chinese_char) {
                '_'
            } else {
                '\0'
            })
        })
    {
        let mut name = name
            .trim()
            .trim_start_matches(['-', ' ', '_'])
            .trim_end_matches([' ', '_'])
            .replace('_', " ");
        if name.ends_with(" -") {
            name.truncate(name.len() - 2);
        }
        if let Some(cap) = SEASON.captures(&name) {
            season = season
                .or_else(|| cap.get(1).and_then(|v| cn_number_to_i64(v.as_str())))
                .or_else(|| {
                    cap.get(2)
                        .or_else(|| cap.get(3))
                        .and_then(|v| v.as_str().parse().ok())
                });
            name.truncate(cap.get(0).unwrap().start());
        }
        if let Some(cap) = YEAR.captures(&name) {
            let number = &cap[1];
            let parenthesized = cap[0].contains(['(', '（']);
            // 同一个数字同时属于中文别名，或来自带独立集号的文件名时，保留为片名。
            let title_number = !parenthesized
                && (meta.isfile
                    || names.iter().any(|alias| {
                        alias.split('/').any(|part| {
                            part.chars().any(is_chinese_char) && part.trim().ends_with(number)
                        })
                    }));
            if title_number {
                if meta.year.as_deref() == Some(number) {
                    meta.year = None;
                }
            } else {
                meta.year = Some(number.to_string());
                name.truncate(cap.get(0).unwrap().start());
            }
        }
        let name = name.trim();
        if name.is_empty()
            || is_label(name)
            || name
                .chars()
                .any(|ch| ('\u{3040}'..='\u{30ff}').contains(&ch))
        {
            continue;
        }
        if name.chars().any(is_chinese_char) {
            if let Some(cap) = MIXED_ALIAS.captures(name).filter(|cap| {
                cap[2].split_whitespace().count() > 1
                    && !names.iter().flat_map(|part| part.split('/')).any(|part| {
                        !part.chars().any(is_chinese_char) && part.trim().ends_with(cap[2].trim())
                    })
            }) {
                chinese.get_or_insert_with(|| cap[1].trim().to_string());
                latin = Some(cap[2].trim().to_string());
            } else {
                chinese.get_or_insert_with(|| name.to_string());
            }
        } else if name.chars().any(|ch| ch.is_alphabetic()) {
            latin = Some(name.to_string());
        }
    }
    if chinese.is_some() || latin.is_some() {
        meta.cn_name = chinese;
        if let Some(latin) = latin {
            meta.en_name = Some(super::parser::to_title_case(&latin));
        } else if names.iter().any(|name| name.contains('/')) {
            // 全中文别名不能留下 anitomy 截出的单个 X 等伪英文名。
            meta.en_name = None;
        }
        if non_episode_file {
            meta.begin_episode = None;
            meta.end_episode = None;
            meta.total_episode = 0;
            meta.media_type = "未知".to_string();
        }
        // 裸技术参数之前没有集号；旧分词器会把 10-bit、30-sai 当作 E10/E30。
        if bare_tech_boundary && !EXPLICIT_SEASON.is_match(title) {
            meta.begin_episode = None;
            meta.end_episode = None;
            meta.total_episode = 0;
            meta.begin_season = None;
            meta.end_season = None;
            meta.total_season = 0;
            meta.media_type = "未知".to_string();
            let suffix = &title[title.len() - rest.len()..];
            let mut seasons = Vec::new();
            for block in suffix
                .split(['[', '【'])
                .skip(1)
                .filter_map(|s| s.split([']', '】']).next())
            {
                for cap in BATCH_SEASON.captures_iter(block) {
                    seasons.extend(
                        [cap.get(1), cap.get(2)]
                            .into_iter()
                            .flatten()
                            .filter_map(|v| v.as_str().parse::<i64>().ok()),
                    );
                }
            }
            if let (Some(begin), Some(end)) = (seasons.iter().min(), seasons.iter().max()) {
                meta.begin_season = Some(*begin);
                meta.end_season = (*end > *begin).then_some(*end);
                meta.total_season = end - begin + 1;
                meta.media_type = "电视剧".to_string();
            } else if MOVIE_BATCH.is_match(title) {
                meta.media_type = "电影".to_string();
            }
        }
        if !EXPLICIT_SEASON.is_match(title) {
            if let Some(season) = season {
                meta.begin_season = Some(season);
                meta.end_season = None;
                meta.total_season = 1;
                meta.media_type = "电视剧".to_string();
            }
        }
        if meta.resource_team.is_none() && !is_label(group) {
            meta.resource_team = Some(group.to_string());
        }
        if meta.resource_pix.is_none() {
            // 普通影视分词器会忽略括号内编码参数，结构已确认后补取明确分辨率。
            meta.resource_pix = RESOLUTION.captures_iter(title).find_map(|cap| {
                let value = cap[1].to_ascii_lowercase();
                if let Some((_, height)) = value.split_once('x') {
                    Some(format!("{}p", height.trim_end_matches(['p', 'i'])))
                } else if value.ends_with(['p', 'i']) {
                    Some(value)
                } else {
                    None
                }
            });
        }
        return true;
    }
    false
}
