// 切块（拆书 stage 0）：把全文切为段落，供逐段生成结构化笔记。
// 纯确定性预处理：禁止 LLM、禁止语义判断、同输入同输出。规范见 docs/工具实现/自动拆书如何实现.md。

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// 切块算法身份标识（规范 §8：下游缓存键必须包含它；V1 任务不跨次缓存，缓存能力落地时启用）
#[allow(dead_code)]
pub const CHUNKER_VERSION: &str = "v1";

// ---- 常量（规范 §3） ----

const MAX_HEADING_LINE_CHARS: usize = 80;
const MIN_HEADING_COUNT: usize = 3;
const MIN_CHAPTER_BODY_CHARS: usize = 120;
const TARGET_SEGMENT_COUNT: usize = 12;
const TARGET_SEGMENT_CHARS: usize = 10_000;
const MIN_SEGMENT_CHARS: usize = 6_000;
const MAX_SEGMENT_CHARS: usize = 16_000;
const CHUNK_OVERLAP_CHARS: usize = 400;
const MAX_MERGED_SEGMENT_CHARS: usize = 48_000;
/// 二次切分的换行边界最小位置：避免切在标题行自身的换行上产生「仅标题」碎段
const RESPLIT_MIN_CUT_CHARS: usize = 200;

// ---- 标题模式（规范 §5.1；有序、只追加不改算法） ----

static HEADING_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        // 中文网文：序章 / 楔子 / 尾声 / 后记 / 番外 / 第X章(节|回|卷|部|集|篇)
        Regex::new(r"^(\s*)((序章|楔子|尾声|后记|番外|第[零一二三四五六七八九十百千万两\d]+[章节回卷部集篇])[^\n]{0,40})\s*$").unwrap(),
        // 英文章节：chapter 12 / chap. 3
        Regex::new(r"^(\s*)((chapter\s+\d+|chap\.\s*\d+)[^\n]{0,40})\s*$").unwrap(),
        // 剧本场次：第X场(幕) / 场3 / S3 / INT. / EXT. / 内景 / 外景 场景行（允许地点/时间后缀）
        Regex::new(r"^(\s*)((第[零一二三四五六七八九十百千万两\d]+[场幕]|场\s*\d+|S\s*\d+|(INT\.|EXT\.|内景|外景))[^\n]{0,60})\s*$").unwrap(),
    ]
});

// ---- 输入输出契约（规范 §2） ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceSegment {
    /// 段落标识：章节/场次标题原文，或「片段 N」
    pub label: String,
    /// 段落正文（已 trim）
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChunkStats {
    pub strategy: String, // "chapter" | "plain"
    pub segment_count: usize,
    pub covered_char_length: usize,
    pub total_char_length: usize,
    pub discarded_chapter_count: usize,
    pub discarded_chapter_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChunkResult {
    pub segments: Vec<SourceSegment>,
    pub stats: ChunkStats,
}

// ---- 总流程（规范 §4） ----

pub fn build_chunks(content: &str) -> ChunkResult {
    // 1. 统一换行符
    let content = if content.contains('\r') {
        content.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        content.to_string()
    };
    let total = content.chars().count();

    // 2. 标题检测
    let headings = detect_headings(&content);

    // 3. 路径选择
    let (segments, mut stats) = if headings.len() >= MIN_HEADING_COUNT {
        let (segs, discarded_count, discarded_chars) = split_by_chapters(&content, &headings);
        let discarded = (discarded_count, discarded_chars);
        if segs.len() >= MIN_HEADING_COUNT {
            // 5.3 章节合并
            (merge_segments(segs), Stats::chapter(total, discarded))
        } else {
            // 过滤后不足 3 段：章节路径整体放弃，过滤统计保留（规范 §5.2）
            let (plain, _, _) = split_plain(&content);
            (plain, Stats::plain(total, discarded))
        }
    } else {
        let (plain, _, _) = split_plain(&content);
        (plain, Stats::plain(total, (0, 0)))
    };

    // 段数不设上限：过多段由用户在选择闸门中挑选（见角色卡提炼规范）
    stats.segment_count = segments.len();
    ChunkResult {
        segments,
        stats,
    }
}

struct Stats;

impl Stats {
    fn chapter(total: usize, discarded: (usize, usize)) -> ChunkStats {
        ChunkStats {
            strategy: "chapter".into(),
            segment_count: 0,
            covered_char_length: total,
            total_char_length: total,
            discarded_chapter_count: discarded.0,
            discarded_chapter_chars: discarded.1,
        }
    }
    fn plain(total: usize, discarded: (usize, usize)) -> ChunkStats {
        ChunkStats {
            strategy: "plain".into(),
            segment_count: 0,
            covered_char_length: total,
            total_char_length: total,
            discarded_chapter_count: discarded.0,
            discarded_chapter_chars: discarded.1,
        }
    }
}

/// 逐行扫描收集 {行号, 标题原文}；标题行为 trim 后原文
pub(crate) fn detect_headings(content: &str) -> Vec<(usize, String)> {
    content
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            let t = line.trim();
            !t.is_empty() && t.chars().count() <= MAX_HEADING_LINE_CHARS && is_heading(t)
        })
        .map(|(i, line)| (i, line.trim().to_string()))
        .collect()
}

fn is_heading(line: &str) -> bool {
    HEADING_PATTERNS.iter().any(|re| re.is_match(line))
}

/// 剧本场次标题判定（探测与引用单元表复用：与切块第 3 类模式一致，允许地点/时间后缀）
pub(crate) fn is_scene_heading(line: &str) -> bool {
    static SCENE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(第[零一二三四五六七八九十百千\d]+[场幕]|场\s*\d+|S\s*\d+|(INT\.|EXT\.|内景|外景))[^\n]{0,60}$")
            .unwrap()
    });
    let t = line.trim();
    !t.is_empty() && t.chars().count() <= MAX_HEADING_LINE_CHARS && SCENE.is_match(t)
}

// ---- 章节路径（规范 §5.2 / §5.3） ----

type RawSeg = (String, String); // (label, content)

fn split_by_chapters(content: &str, headings: &[(usize, String)]) -> (Vec<RawSeg>, usize, usize) {
    let lines: Vec<&str> = content.lines().collect();
    let mut segs = Vec::new();
    let mut discarded_count = 0;
    let mut discarded_chars = 0;
    for (i, (line_idx, label)) in headings.iter().enumerate() {
        let start = *line_idx;
        let end = headings.get(i + 1).map(|(e, _)| *e).unwrap_or(lines.len());
        let text = lines[start..end].join("\n");
        let body_chars = text.trim().chars().count();
        // 整段（含标题行）< 120 字 → 过滤，计入统计（不静默）
        if body_chars < MIN_CHAPTER_BODY_CHARS {
            discarded_count += 1;
            discarded_chars += body_chars;
            continue;
        }
        segs.push((label.clone(), text.trim().to_string()));
    }
    (segs, discarded_count, discarded_chars)
}

fn merge_segments(chapters: Vec<RawSeg>) -> Vec<SourceSegment> {
    let n = chapters.len();
    if n <= TARGET_SEGMENT_COUNT {
        return finalize(
            chapters
                .into_iter()
                .map(|(label, content)| SourceSegment { label, content })
                .collect(),
        );
    }
    let group = n.div_ceil(TARGET_SEGMENT_COUNT);
    let mut merged = Vec::new();
    let mut idx = 0;
    while idx < n {
        let group_items = &chapters[idx..std::cmp::min(idx + group, n)];
        let label = if group_items.len() == 1 {
            group_items[0].0.clone()
        } else {
            format!("{} ~ {}", group_items[0].0, group_items.last().unwrap().0)
        };
        let content = group_items
            .iter()
            .map(|(_, c)| c.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        merged.push(SourceSegment { label, content });
        idx += group;
    }
    finalize(merged)
}

/// 合并段超上限 → 按换行边界二次切分，label 追加（一）（二）…（规范 §5.3 大小检查）
fn finalize(segs: Vec<SourceSegment>) -> Vec<SourceSegment> {
    let mut out = Vec::new();
    for seg in segs {
        if seg.content.chars().count() <= MAX_MERGED_SEGMENT_CHARS {
            out.push(seg);
            continue;
        }
        let mut rest = seg.content.as_str();
        let mut part = 1usize;
        while !rest.is_empty() {
            let limit = rest
                .char_indices()
                .nth(MAX_MERGED_SEGMENT_CHARS)
                .map(|(i, _)| i)
                .unwrap_or(rest.len());
            let mut cut = limit;
            if cut < rest.len()
                && let Some(nl) = rest[..limit].rfind('\n')
                    && nl >= RESPLIT_MIN_CUT_CHARS {
                        cut = nl;
                    }
            let (chunk, remainder) = rest.split_at(cut);
            let label = if part == 1 {
                seg.label.clone()
            } else {
                let cn = ['一', '二', '三', '四', '五', '六', '七', '八', '九'];
                let suffix = if part <= 9 {
                    cn[part - 1].to_string()
                } else {
                    part.to_string()
                };
                format!("{}（{suffix}）", seg.label)
            };
            let trimmed = chunk.trim();
            if !trimmed.is_empty() {
                out.push(SourceSegment {
                    label,
                    content: trimmed.to_string(),
                });
            }
            part += 1;
            rest = remainder.trim_start_matches('\n');
        }
    }
    out
}

// ---- 字符切块路径（规范 §6 滑窗） ----

fn split_plain(content: &str) -> (Vec<SourceSegment>, usize, usize) {
    let chars: Vec<char> = content.chars().collect();
    let total = chars.len();
    let mut segs = Vec::new();
    let mut start = 0usize;
    let mut order = 1;
    while start < total {
        let target_end = std::cmp::min(start + TARGET_SEGMENT_CHARS, total);
        let mut boundary = target_end;
        if target_end < total {
            // 在 min(start+16k, 文末) 处向前找最后一个换行；candidate > start+6k 才采用
            let search_limit = std::cmp::min(start + MAX_SEGMENT_CHARS, total);
            let mut candidate = None;
            let mut j = search_limit;
            while j > start {
                if chars[j - 1] == '\n' {
                    candidate = Some(j);
                    break;
                }
                j -= 1;
            }
            if let Some(c) = candidate
                && c > start + MIN_SEGMENT_CHARS {
                    boundary = c;
                }
        }
        let chunk: String = chars[start..boundary].iter().collect();
        let trimmed = chunk.trim();
        if !trimmed.is_empty() {
            segs.push(SourceSegment {
                label: format!("片段 {order}"),
                content: trimmed.to_string(),
            });
            order += 1;
        }
        if boundary >= total {
            break;
        }
        start = std::cmp::max(boundary.saturating_sub(CHUNK_OVERLAP_CHARS), start + 1);
    }
    (segs, 0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heading(title: &str, body_chars: usize) -> String {
        let body = "正".repeat(body_chars);
        format!("{title}\n{body}\n")
    }

    #[test]
    fn chapter_split_and_merge() {
        // 24 章 > 12 目标 → 两章合一段
        let mut text = String::new();
        for i in 1..=24 {
            text.push_str(&heading(&format!("第{i}章 测试"), 200));
        }
        let r = build_chunks(&text);
        assert_eq!(r.stats.strategy, "chapter");
        assert_eq!(r.segments.len(), 12);
        assert!(r.segments[0].label.contains('~'));
    }

    #[test]
    fn chapter_few_stays_unmerged() {
        let mut text = String::new();
        for i in 1..=5 {
            text.push_str(&heading(&format!("第{i}章"), 200));
        }
        let r = build_chunks(&text);
        assert_eq!(r.segments.len(), 5);
        assert_eq!(r.segments[0].label, "第1章");
    }

    #[test]
    fn short_chapters_filtered() {
        let mut text = String::new();
        text.push_str(&heading("第1章", 200));
        text.push_str(&heading("第2章", 50)); // 过短，过滤但计入统计
        text.push_str(&heading("第3章", 200));
        text.push_str(&heading("第4章", 200));
        let r = build_chunks(&text);
        assert_eq!(r.stats.discarded_chapter_count, 1);
        assert!(r.segments.iter().all(|s| s.label != "第2章"));
    }

    #[test]
    fn scene_headings_detected() {
        let mut text = String::new();
        for i in 1..=6 {
            text.push_str(&heading(&format!("第{i}场"), 300));
        }
        let r = build_chunks(&text);
        assert_eq!(r.stats.strategy, "chapter");
        assert_eq!(r.segments.len(), 6);

        let mut text2 = String::new();
        for i in 1..=6 {
            text2.push_str(&heading(&format!("INT. 房间 {i}"), 300));
        }
        let r2 = build_chunks(&text2);
        assert_eq!(r2.stats.strategy, "chapter");
        assert_eq!(r2.segments.len(), 6);
    }

    #[test]
    fn plain_sliding_window() {
        let para = "这是一段没有换行的长正文用于测试滑窗行为。".repeat(60); // ~1000 字
        let mut text = String::new();
        for i in 1..=30 {
            text.push_str(&format!("段落{i}开始\n{para}\n\n"));
        }
        let r = build_chunks(&text);
        assert_eq!(r.stats.strategy, "plain");
        assert!(r.segments.len() >= 2, "约 3 万字应切成多段");
        assert_eq!(r.segments[0].label, "片段 1");
    }

    #[test]
    fn plain_boundary_at_newline() {
        // 目标 1 万字处附近应有换行可回退
        let a = "甲".repeat(6000);
        let b = "乙".repeat(200);
        let c = "丙".repeat(6000);
        let text = format!("{a}\n{b}\n{c}");
        let r = build_chunks(&text);
        assert_eq!(r.stats.strategy, "plain");
        assert!(r.segments.len() >= 2);
        for seg in &r.segments {
            assert!(seg.content.chars().count() <= MAX_SEGMENT_CHARS);
        }
        // 第一段边界应落在换行：内容以完整「乙」或「丙」行开头（未硬切）
        let second = &r.segments[1];
        assert!(
            second.content.starts_with("乙") || second.content.starts_with("丙") || second.content.starts_with("甲"),
            "边界应落在换行上，实际开头：{}",
            &second.content[..second.content.len().min(10)]
        );
    }

    #[test]
    fn no_segment_cap() {
        // 无章节结构的超长文本：不再设段数上限，全量切出（用户在选择闸门挑选）
        let text = "正".repeat(300_000);
        let r = build_chunks(&text);
        assert!(r.segments.len() > 24, "实际段数 {}", r.segments.len());
    }

    #[test]
    fn deterministic() {
        let text: String = (1..=15)
            .map(|i| heading(&format!("第{i}章"), 300))
            .collect();
        let a = build_chunks(&text);
        let b = build_chunks(&text);
        assert_eq!(a, b);
    }

    #[test]
    fn false_positive_heading_ignored() {
        // 正文长句含「第X章」字样但行超长 → 不判为标题
        let long_line = format!("他说到第三章的内容并且这句话非常长{}", "很长".repeat(60));
        let text = format!("{long_line}\n{}", "正文".repeat(400));
        let r = build_chunks(&text);
        assert_eq!(r.stats.strategy, "plain");
    }

    #[test]
    fn merged_oversize_resplit() {
        // 章节路径：第1章 6 万字 > 48k → 单章二次切分；第2/3章保证走章节路径（≥3 标题）
        let mut text = heading("第1章", 60_000);
        text.push_str(&heading("第2章", 200));
        text.push_str(&heading("第3章", 200));
        let r = build_chunks(&text);
        assert_eq!(r.stats.strategy, "chapter");
        assert!(r.segments.len() >= 4);
        assert_eq!(r.segments[0].label, "第1章");
        assert!(r.segments[1].label.contains("（二）"));
        assert!(r.segments.iter().all(|s| {
            s.content.chars().count() <= MAX_MERGED_SEGMENT_CHARS
        }));
    }

    #[test]
    fn crlf_normalized() {
        let mut text = String::new();
        for i in 1..=4 {
            let body = "正".repeat(200);
            text.push_str(&format!("第{i}章\r\n{body}\r\n"));
        }
        let r = build_chunks(&text.replace('\n', "\r\n"));
        assert_eq!(r.stats.strategy, "chapter");
        assert_eq!(r.segments.len(), 4);
    }
}
