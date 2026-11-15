use super::parser::{cn_number_to_i64, i64_to_cn_number, is_chinese_char};
use super::patterns::{LEADING_ZERO_RE, SUBTITLE_EPISODE_RANGE_FIN_RE};
use super::regex::{Regex, RegexBuilder};
use fancy_regex::Captures;
use once_cell::sync::Lazy;
use std::collections::HashMap;

/// 随解析选项保存整份词表的规则与正则，避免逐条 LRU 淘汰导致循环重编译。
#[derive(Default)]
pub(super) struct PreparedWords {
    rules: Vec<(String, String, Vec<String>)>,
    regexes: HashMap<String, Option<Regex>>,
}

impl PreparedWords {
    /// 一次解析并编译词表；无效正则也保留结果，后续标题不再重复编译。
    pub(super) fn new(words: &[String]) -> Self {
        let mut prepared = Self::default();
        for word in words {
            if word.is_empty() || word.starts_with('#') {
                continue;
            }
            let Some((word_type, params)) = parse_custom_word(word) else {
                continue;
            };
            let mut patterns = Vec::new();
            match word_type.as_str() {
                "replace_and_offset" => {
                    patterns.push(params[0].clone());
                    patterns.extend(offset_patterns(&params[2], &params[3]));
                }
                "offset" => patterns.extend(offset_patterns(&params[0], &params[1])),
                _ => patterns.push(params[0].clone()),
            }
            for pattern in patterns {
                prepared
                    .regexes
                    .entry(pattern.clone())
                    .or_insert_with(|| Regex::new(&pattern).ok());
            }
            prepared.rules.push((word.clone(), word_type, params));
        }
        prepared
    }

    /// 借用词表持有的编译结果，解析期间无需全局锁或 LRU 顺序更新。
    fn regex(&self, pattern: &str) -> Option<&Regex> {
        self.regexes.get(pattern)?.as_ref()
    }
}

/// 收集集数偏移使用的定位正则与完整匹配正则。
fn offset_patterns(front: &str, back: &str) -> Vec<String> {
    vec![
        front.to_string(),
        back.to_string(),
        offset_pattern(front, back),
    ]
}

/// 统一编译与执行阶段的集数偏移匹配表达式。
fn offset_pattern(front: &str, back: &str) -> String {
    format!(
        r"(?<={}.*?)[0-9一二三四五六七八九十]+(?=.*?{})",
        front, back
    )
}

static SUBTITLE_EPISODE_SPAN_RE: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(
        r"(?<![0-9一二三四五六七八九十百零])([0-9一二三四五六七八九十百零]+)\s*(?:[集话話期幕]?\s*[-~～–—至]\s*第?\s*([0-9一二三四五六七八九十百零]+)\s*)?[集话話期幕](?!\s*[全共](?!\s*[0-9一二三四五六七八九十百零]))",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
});
static SUBTITLE_EPISODE_TITLE_RE: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r"(?<![A-Za-z0-9_])Episode\s+(\d{1,4})(?![A-Za-z0-9_])")
        .case_insensitive(true)
        .build()
        .unwrap()
});
static SUBTITLE_EPISODE_TOKEN_RE: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(
        r"(?<![A-Za-z0-9_])(?:EP|E)(\d{1,4})(?:\s*[-~～–—]\s*(?:EP|E)?(\d{1,4}))?(?![A-Za-z0-9_])",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
});

/// 应用自定义识别词，覆盖替换、屏蔽和集数偏移三类规则。
pub(super) fn prepare_words(
    title: &str,
    subtitle: Option<&str>,
    words: &PreparedWords,
) -> (String, Option<String>, Vec<String>) {
    let mut title = title.to_string();
    let mut subtitle = subtitle.map(str::to_string);
    let mut applied = Vec::new();
    for (word, word_type, params) in &words.rules {
        let mut state = false;
        match word_type.as_str() {
            "replace_and_offset" => {
                let (new_title, replace_state) =
                    replace_regex(&title, &params[0], &params[1], words);
                title = new_title;
                if replace_state {
                    let (new_title, new_subtitle, offset_state) = episode_offset_with_subtitle(
                        &title,
                        subtitle.as_deref(),
                        &params[2],
                        &params[3],
                        &params[4],
                        words,
                    );
                    title = new_title;
                    subtitle = new_subtitle;
                    state = offset_state;
                }
            }
            "replace" => {
                let (new_title, replace_state) =
                    replace_regex(&title, &params[0], &params[1], words);
                title = new_title;
                state = replace_state;
            }
            "offset" => {
                let (new_title, new_subtitle, offset_state) = episode_offset_with_subtitle(
                    &title,
                    subtitle.as_deref(),
                    &params[0],
                    &params[1],
                    &params[2],
                    words,
                );
                title = new_title;
                subtitle = new_subtitle;
                state = offset_state;
            }
            _ => {
                let (new_title, replace_state) = replace_regex(&title, &params[0], "", words);
                title = new_title;
                state = replace_state;
            }
        }
        if state {
            applied.push(word.clone());
        }
    }
    (title, subtitle, applied)
}

/// 解析自定义识别词格式。
fn parse_custom_word(word: &str) -> Option<(String, Vec<String>)> {
    if word.contains(" => ")
        && word.contains(" && ")
        && word.contains(" >> ")
        && word.contains(" <> ")
    {
        static COMBINED_WORD_RE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^\s*(.*?)\s*=>\s*(.*?)\s*&&\s*(.*?)\s*<>\s*(.*?)\s*>>\s*(.*?)\s*$")
                .unwrap()
        });
        return COMBINED_WORD_RE.captures(word).map(|cap| {
            (
                "replace_and_offset".to_string(),
                (1..=5)
                    .map(|index| cap.get(index).unwrap().as_str().trim().to_string())
                    .collect(),
            )
        });
    }
    if word.contains(" => ") {
        let parts = word.split(" => ").map(str::to_string).collect::<Vec<_>>();
        return Some(("replace".to_string(), parts));
    }
    if word.contains(" >> ") && word.contains(" <> ") {
        let mut parts = word.split(" <> ").map(str::to_string).collect::<Vec<_>>();
        if parts.len() < 2 {
            return None;
        }
        let offsets = parts[1]
            .split(" >> ")
            .map(str::to_string)
            .collect::<Vec<_>>();
        if offsets.len() < 2 {
            return None;
        }
        parts[1] = offsets[0].clone();
        parts.push(offsets[1].clone());
        return Some(("offset".to_string(), parts));
    }
    (!word.trim().is_empty()).then(|| ("block".to_string(), vec![word.to_string()]))
}

/// 执行自定义识别词正则替换。
fn replace_regex(
    title: &str,
    replaced: &str,
    replacement: &str,
    words: &PreparedWords,
) -> (String, bool) {
    let Some(regex) = words.regex(replaced) else {
        return (title.to_string(), false);
    };
    let Ok(result) = regex.inner.try_replacen(title, 0, |cap: &Captures<'_>| {
        expand_python_replacement(cap, replacement)
    }) else {
        return (title.to_string(), false);
    };
    let state = matches!(result, std::borrow::Cow::Owned(_));
    (result.into_owned(), state)
}

/// 执行自定义识别词集数偏移。
fn episode_offset(
    title: &str,
    front: &str,
    back: &str,
    offset: &str,
    words: &PreparedWords,
) -> (String, bool) {
    if !back.is_empty()
        && words
            .regex(back)
            .map(|regex| regex.is_match(title))
            .map(|matched| !matched)
            .unwrap_or(true)
    {
        return (title.to_string(), false);
    }
    if !front.is_empty()
        && words
            .regex(front)
            .map(|regex| regex.is_match(title))
            .map(|matched| !matched)
            .unwrap_or(true)
    {
        return (title.to_string(), false);
    }
    let pattern = offset_pattern(front, back);
    let Some(regex) = words.regex(&pattern) else {
        return (title.to_string(), false);
    };
    let mut replaced = false;
    let Ok(result) = regex.inner.try_replacen(title, 0, |cap: &Captures<'_>| {
        let Some(value) = cap.get(0).map(|item| item.as_str()) else {
            return String::new();
        };
        let Some(number) = cn_number_to_i64(value) else {
            return value.to_string();
        };
        let Some(offset_value) = eval_episode_offset(offset, number) else {
            return value.to_string();
        };
        replaced = true;
        format_episode_offset(value, offset_value)
    }) else {
        return (title.to_string(), false);
    };
    if replaced {
        (result.into_owned(), true)
    } else {
        (title.to_string(), false)
    }
}

/// 在标题和副标题中按现有规则尝试集数偏移，并保留两个字段的边界。
fn episode_offset_with_subtitle(
    title: &str,
    subtitle: Option<&str>,
    front: &str,
    back: &str,
    offset: &str,
    words: &PreparedWords,
) -> (String, Option<String>, bool) {
    let (new_title, title_state) = episode_offset(title, front, back, offset, words);
    if title_state {
        return (new_title, subtitle.map(str::to_string), true);
    }
    let Some(subtitle) = subtitle else {
        return (title.to_string(), None, false);
    };
    let (new_subtitle, subtitle_state) = episode_offset(subtitle, front, back, offset, words);
    if subtitle_state {
        return (title.to_string(), Some(new_subtitle), true);
    }
    let context = format!("{title} {subtitle}");
    if !locators_match(&context, front, back, words) {
        return (title.to_string(), Some(subtitle.to_string()), false);
    }
    let (new_subtitle, subtitle_state) = episode_offset_subtitle(subtitle, offset);
    if subtitle_state {
        return (title.to_string(), Some(new_subtitle), true);
    }
    (title.to_string(), Some(subtitle.to_string()), false)
}

/// 判断定位词是否在标题和副标题组成的上下文中同时出现。
fn locators_match(text: &str, front: &str, back: &str, words: &PreparedWords) -> bool {
    for locator in [front, back] {
        if !locator.is_empty()
            && !words
                .regex(locator)
                .map(|regex| regex.is_match(text))
                .unwrap_or(false)
        {
            return false;
        }
    }
    true
}

/// 收集副标题中会被 MetaInfo 识别为集数的数字范围。
fn subtitle_episode_spans(subtitle: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    for captures in SUBTITLE_EPISODE_RANGE_FIN_RE.captures_iter(subtitle) {
        let begin = captures
            .get(1)
            .and_then(|item| item.as_str().parse::<i64>().ok());
        let end = captures
            .get(2)
            .and_then(|item| item.as_str().parse::<i64>().ok());
        if let (Some(begin), Some(end)) = (begin, end) {
            if begin >= 1 && end >= begin && end < 10000 && !(1900..=2155).contains(&begin) {
                push_capture_span(&mut spans, &captures, 1);
                push_capture_span(&mut spans, &captures, 2);
            }
        }
    }
    for captures in SUBTITLE_EPISODE_SPAN_RE.captures_iter(subtitle) {
        let Some(full_match) = captures.get(0) else {
            continue;
        };
        let prefix = subtitle[..full_match.start()].trim_end();
        let before_ordinal = prefix
            .strip_suffix("EP")
            .or_else(|| prefix.strip_suffix("ep"))
            .or_else(|| prefix.strip_suffix('E'))
            .or_else(|| prefix.strip_suffix('e'))
            .map(str::trim_end)
            .unwrap_or(prefix);
        let before_di = before_ordinal
            .strip_suffix('第')
            .map(str::trim_end)
            .unwrap_or(before_ordinal);
        if prefix.ends_with(['全', '共'])
            || before_di.ends_with(['全', '共'])
            || before_di.ends_with("不包含")
            || before_di.ends_with("不包括")
            || before_di.ends_with("不含")
        {
            continue;
        }
        push_capture_span(&mut spans, &captures, 1);
        push_capture_span(&mut spans, &captures, 2);
    }
    for captures in SUBTITLE_EPISODE_TITLE_RE.captures_iter(subtitle) {
        push_capture_span(&mut spans, &captures, 1);
    }
    for captures in SUBTITLE_EPISODE_TOKEN_RE.captures_iter(subtitle) {
        push_capture_span(&mut spans, &captures, 1);
        push_capture_span(&mut spans, &captures, 2);
    }
    spans.sort_unstable_by_key(|(start, _)| *start);
    spans
}

/// 添加不与已有范围重叠的正则捕获范围。
fn push_capture_span(spans: &mut Vec<(usize, usize)>, captures: &Captures<'_>, index: usize) {
    let Some(value) = captures.get(index) else {
        return;
    };
    let span = (value.start(), value.end());
    if !spans
        .iter()
        .any(|(start, end)| span.0 < *end && span.1 > *start)
    {
        spans.push(span);
    }
}

/// 只偏移副标题中的集数表达式，避免修改副标题里的季数或年份。
fn episode_offset_subtitle(subtitle: &str, offset: &str) -> (String, bool) {
    let spans = subtitle_episode_spans(subtitle);
    if spans.is_empty() {
        return (subtitle.to_string(), false);
    }
    let mut replacements = Vec::with_capacity(spans.len());
    for (start, end) in &spans {
        let value = &subtitle[*start..*end];
        let Some(number) = cn_number_to_i64(value) else {
            return (subtitle.to_string(), false);
        };
        let Some(offset_value) = eval_episode_offset(offset, number) else {
            return (subtitle.to_string(), false);
        };
        replacements.push((*start, *end, format_episode_offset(value, offset_value)));
    }
    let mut result = subtitle.to_string();
    for (start, end, replacement) in replacements.into_iter().rev() {
        result.replace_range(start..end, &replacement);
    }
    (result, true)
}

/// 按 Python regex.sub 的反斜杠分组语义展开替换词。
fn expand_python_replacement(cap: &Captures<'_>, replacement: &str) -> String {
    let mut result = String::new();
    let mut chars = replacement.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        let Some(next) = chars.next() else {
            result.push('\\');
            break;
        };
        if next.is_ascii_digit() {
            let mut index = next.to_digit(10).unwrap_or(0) as usize;
            while let Some(peek) = chars.peek().copied().filter(char::is_ascii_digit) {
                chars.next();
                index = index * 10 + peek.to_digit(10).unwrap_or(0) as usize;
            }
            if let Some(group) = cap.get(index) {
                result.push_str(group.as_str());
            }
        } else {
            result.push(next);
        }
    }
    result
}

/// 按原集数字符串格式返回偏移后的集数字符串。
fn format_episode_offset(value: &str, offset_value: i64) -> String {
    if value.chars().any(is_chinese_char) {
        return i64_to_cn_number(offset_value);
    }
    if LEADING_ZERO_RE.find(value).is_none() {
        return offset_value.to_string();
    }
    let width = value.len();
    if offset_value < 0 {
        return format!("-{:0width$}", offset_value.saturating_abs(), width = width);
    }
    format!("{:0width$}", offset_value, width = width)
}

/// 集数偏移算术表达式的词法单元。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EpisodeOffsetToken {
    Number(i64),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LeftParen,
    RightParen,
}

/// 计算 EP 偏移表达式，支持数字、EP、括号和基础算术运算符。
fn eval_episode_offset(expr: &str, episode: i64) -> Option<i64> {
    let tokens = tokenize_episode_offset(expr, episode)?;
    let mut parser = EpisodeOffsetParser {
        tokens: &tokens,
        index: 0,
    };
    let value = parser.parse_expression()?;
    (parser.index == tokens.len()).then_some(value)
}

/// 将 EP 偏移表达式拆成可安全计算的 token。
fn tokenize_episode_offset(expr: &str, episode: i64) -> Option<Vec<EpisodeOffsetToken>> {
    let chars = expr.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];
        if ch.is_whitespace() {
            index += 1;
            continue;
        }
        if ch.is_ascii_digit() {
            let start = index;
            index += 1;
            while index < chars.len() && chars[index].is_ascii_digit() {
                index += 1;
            }
            let value = chars[start..index]
                .iter()
                .collect::<String>()
                .parse()
                .ok()?;
            tokens.push(EpisodeOffsetToken::Number(value));
            continue;
        }
        if ch == 'E' && chars.get(index + 1) == Some(&'P') {
            tokens.push(EpisodeOffsetToken::Number(episode));
            index += 2;
            continue;
        }
        let token = match ch {
            '+' => EpisodeOffsetToken::Plus,
            '-' => EpisodeOffsetToken::Minus,
            '*' => EpisodeOffsetToken::Star,
            '/' => EpisodeOffsetToken::Slash,
            '%' => EpisodeOffsetToken::Percent,
            '(' => EpisodeOffsetToken::LeftParen,
            '\\' => return None,
            ')' => EpisodeOffsetToken::RightParen,
            _ => return None,
        };
        tokens.push(token);
        index += 1;
    }
    (!tokens.is_empty()).then_some(tokens)
}

/// 按算术优先级计算集数偏移的解析状态。
struct EpisodeOffsetParser<'a> {
    tokens: &'a [EpisodeOffsetToken],
    index: usize,
}

impl EpisodeOffsetParser<'_> {
    /// 解析加减表达式。
    fn parse_expression(&mut self) -> Option<i64> {
        let mut value = self.parse_term()?;
        loop {
            match self.peek() {
                Some(EpisodeOffsetToken::Plus) => {
                    self.index += 1;
                    value = value.checked_add(self.parse_term()?)?;
                }
                Some(EpisodeOffsetToken::Minus) => {
                    self.index += 1;
                    value = value.checked_sub(self.parse_term()?)?;
                }
                _ => return Some(value),
            }
        }
    }

    /// 解析乘除和取余表达式。
    fn parse_term(&mut self) -> Option<i64> {
        let mut value = self.parse_factor()?;
        loop {
            match self.peek() {
                Some(EpisodeOffsetToken::Star) => {
                    self.index += 1;
                    value = value.checked_mul(self.parse_factor()?)?;
                }
                Some(EpisodeOffsetToken::Slash) => {
                    self.index += 1;
                    let right = self.parse_factor()?;
                    if right == 0 {
                        return None;
                    }
                    value = value.checked_div(right)?;
                }
                Some(EpisodeOffsetToken::Percent) => {
                    self.index += 1;
                    let right = self.parse_factor()?;
                    if right == 0 {
                        return None;
                    }
                    value = value.checked_rem(right)?;
                }
                _ => return Some(value),
            }
        }
    }

    /// 解析一元正负号和括号。
    fn parse_factor(&mut self) -> Option<i64> {
        match self.next()? {
            EpisodeOffsetToken::Number(value) => Some(value),
            EpisodeOffsetToken::Plus => self.parse_factor(),
            EpisodeOffsetToken::Minus => self.parse_factor()?.checked_neg(),
            EpisodeOffsetToken::LeftParen => {
                let value = self.parse_expression()?;
                (self.next() == Some(EpisodeOffsetToken::RightParen)).then_some(value)
            }
            _ => None,
        }
    }

    /// 查看下一个 token。
    fn peek(&self) -> Option<EpisodeOffsetToken> {
        self.tokens.get(self.index).copied()
    }

    /// 消耗下一个 token。
    fn next(&mut self) -> Option<EpisodeOffsetToken> {
        let token = self.peek()?;
        self.index += 1;
        Some(token)
    }
}

#[cfg(test)]
mod tests {
    use super::{prepare_words, PreparedWords};
    use crate::metainfo::options::ParseOptions;
    use std::collections::HashMap;
    use std::sync::Arc;

    /// 超过旧容量乃至 1024 的词表须完整保存正则，并跨标题复用同一编译对象。
    #[test]
    fn retains_large_working_set_in_cached_options() {
        let words = (0..1200)
            .map(|i| format!("Rule{i}x => Value{i}x"))
            .collect::<Vec<_>>();
        let options = ParseOptions::cached(
            words.clone(),
            Vec::new(),
            String::new(),
            Vec::new(),
            HashMap::new(),
        );
        let regex = options.custom_words.regex("Rule0x").unwrap() as *const _;
        for _ in 0..3 {
            let (title, _, applied) =
                prepare_words("Rule0x Rule1199x", None, &options.custom_words);
            assert_eq!(title, "Value0x Value1199x");
            assert_eq!(applied, vec![words[0].clone(), words[1199].clone()]);
        }
        let reused =
            ParseOptions::cached(words, Vec::new(), String::new(), Vec::new(), HashMap::new());
        assert!(Arc::ptr_eq(&options, &reused));
        assert_eq!(
            reused.custom_words.regex("Rule0x").unwrap() as *const _,
            regex
        );
        assert_eq!(reused.custom_words.regexes.len(), 1200);
    }

    /// 词表内容和顺序变化必须映射到不同缓存项，不能共享旧编译结果。
    #[test]
    fn isolates_changed_words_and_preserves_rule_order() {
        let first = ParseOptions::cached(
            vec!["Alpha => Beta".to_string(), "Beta => Gamma".to_string()],
            Vec::new(),
            String::new(),
            Vec::new(),
            HashMap::new(),
        );
        let reordered = ParseOptions::cached(
            vec!["Beta => Gamma".to_string(), "Alpha => Beta".to_string()],
            Vec::new(),
            String::new(),
            Vec::new(),
            HashMap::new(),
        );
        assert!(!Arc::ptr_eq(&first, &reordered));
        assert_eq!(prepare_words("Alpha", None, &first.custom_words).0, "Gamma");
        assert_eq!(
            prepare_words("Alpha", None, &reordered.custom_words).0,
            "Beta"
        );
    }

    /// 无效正则和重复正则在词表构建期处理，不影响后续有效规则。
    #[test]
    fn caches_invalid_and_duplicate_patterns_without_aborting_valid_rules() {
        let prepared = PreparedWords::new(&[
            "[ => invalid".to_string(),
            "Dup => One".to_string(),
            "Dup => Two".to_string(),
        ]);
        assert!(prepared.regex("[").is_none());
        assert_eq!(prepared.regexes.len(), 2);
        let (title, _, applied) = prepare_words("Dup", None, &prepared);
        assert_eq!(title, "One");
        assert_eq!(applied, vec!["Dup => One".to_string()]);
    }

    /// 跨字段偏移只改写副标题中的真实集数，不触碰季号、总集数或否定集数，且标题已偏移时不重复偏移。
    #[test]
    fn offsets_subtitle_episodes_safely_across_variants() {
        let bleach_words = PreparedWords::new(&[
            "BLEACH Thousand-Year Blood War S04 => BLEACH 2004 S02 && S02 <> 1080p >> EP+40"
                .to_string(),
        ]);
        let (title, subtitle, applied) = prepare_words(
            "BLEACH Thousand-Year Blood War S04 1080p Disney+ WEB-DL",
            Some("死神 千年血战篇 -祸进谭- 第四季 第8集 全13集 不含第9集"),
            &bleach_words,
        );
        assert_eq!(title, "BLEACH 2004 S02 1080p Disney+ WEB-DL");
        assert_eq!(
            subtitle.as_deref(),
            Some("死神 千年血战篇 -祸进谭- 第四季 第48集 全13集 不含第9集")
        );
        assert_eq!(applied.len(), 1);

        // 标题本身已偏移集数时，不得再二次偏移副标题。
        let (title_both, sub_both, _) = prepare_words(
            "BLEACH Thousand-Year Blood War S04 08 1080p Disney+ WEB-DL",
            Some("死神 第四季 第8集"),
            &bleach_words,
        );
        assert_eq!(title_both, "BLEACH 2004 S02 48 1080p Disney+ WEB-DL");
        assert_eq!(sub_both.as_deref(), Some("死神 第四季 第8集"));

        // 区间集数与完结范围集数两端同时偏移。
        let range_words = PreparedWords::new(&["S02 <> 1080p >> EP+12".to_string()]);
        let (_, range_sub, range_applied) = prepare_words(
            "Show S02 1080p WEB-DL",
            Some("某剧 第01-02集"),
            &range_words,
        );
        assert_eq!(range_sub.as_deref(), Some("某剧 第13-14集"));
        assert_eq!(range_applied.len(), 1);

        let (_, fin_sub, fin_applied) = prepare_words(
            "Show S02 1080p WEB-DL",
            Some("某剧 [01-12Fin]"),
            &range_words,
        );
        assert_eq!(fin_sub.as_deref(), Some("某剧 [13-24Fin]"));
        assert_eq!(fin_applied.len(), 1);

        let (_, total_only_sub, total_only_applied) =
            prepare_words("Show S02 1080p WEB-DL", Some("某剧 24集全"), &range_words);
        assert_eq!(total_only_sub.as_deref(), Some("某剧 24集全"));
        assert!(total_only_applied.is_empty());
    }
}
