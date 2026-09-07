// 素材探测（structure-probe）：判定台词归属结构（script/prose）并产出引用单元表。
// 规范见 docs/工具实现/角色卡提炼如何实现.md §3。确定性，无 LLM。

use crate::deconstruct::chunk::{self, SourceSegment};
use serde::{Deserialize, Serialize};

/// 归属模式抽样行数上限
const ATTRIBUTION_SAMPLE_LINES: usize = 1000;
/// 判定 explicit 归属的命中率阈值（对非空抽样行）
const ATTRIBUTION_HIT_RATIO: f64 = 0.6;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProbeResult {
    /// script：台词有显式归属结构；prose：无（对白识别退化为笔记阶段 LLM 处理）
    pub form: String,
    /// 抽样命中率（诊断用）
    pub attribution_hit_ratio: f64,
    /// 引用单元表：切块段 label → 段内场次标题列表（无场次细分则为空）
    pub citation_units: Vec<SegmentUnits>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SegmentUnits {
    /// 切块段 label
    pub segment: String,
    /// 段内场次标题（有序、去重；为空表示该段只以段 label 为引用单元）
    pub scenes: Vec<String>,
}

/// 台词归属行判定（两种形态）：
/// A. 「名字：台词」——名字 ≤ 12 字符、不含句读；
/// B. 「【名字】台词」——括号是显式说话人标记，冒号可省
static DIALOGUE_LINE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"^(【?)([^\s：:，。！？…·「」『』()（）]{1,12})】?\s*[：:]\s*(.+)$")
        .unwrap()
});
static DIALOGUE_BRACKET: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"^【([^\s：:，。！？…·「」『』()（）]{1,12})】\s*(.+)$").unwrap()
});

/// 叙述引导词黑名单：这些「名字」实际是叙述动词，不构成台词归属
const NARRATION_WORDS: [&str; 8] = ["他说", "她说", "我说", "你说", "他说道", "她说道", "心想", "暗想"];

/// 判定一行是否为台词归属行，返回 (名字, 台词)
pub(crate) fn parse_dialogue_line(line: &str) -> Option<(String, String)> {
    let t = line.trim();
    let parsed = DIALOGUE_LINE
        .captures(t)
        .map(|c| {
            (
                c.get(2).map(|m| m.as_str().to_string()),
                c.get(3).map(|m| m.as_str().trim().to_string()),
            )
        })
        .or_else(|| {
            DIALOGUE_BRACKET.captures(t).map(|c| {
                (
                    c.get(1).map(|m| m.as_str().to_string()),
                    c.get(2).map(|m| m.as_str().trim().to_string()),
                )
            })
        })?;
    let (name, speech) = parsed;
    let name = name?;
    let speech = speech?;
    if NARRATION_WORDS.contains(&name.as_str()) || speech.is_empty() {
        return None;
    }
    Some((name, speech))
}

/// 素材形态判定（探测阶段：只需原文，不依赖切块）
pub fn probe_form(content: &str) -> (String, f64) {
    // 抽样统计归属命中率（对非空行）
    let non_empty: Vec<&str> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(ATTRIBUTION_SAMPLE_LINES)
        .collect();
    let hits = non_empty
        .iter()
        .filter(|l| parse_dialogue_line(l).is_some())
        .count();
    let ratio = if non_empty.is_empty() {
        0.0
    } else {
        hits as f64 / non_empty.len() as f64
    };
    let form = if ratio >= ATTRIBUTION_HIT_RATIO {
        "script"
    } else {
        "prose"
    };
    (form.into(), (ratio * 1000.0).round() / 1000.0)
}

/// 引用单元表（切块阶段：在切块产物上标注场次）
pub fn citation_units(chunks: &[SourceSegment]) -> Vec<SegmentUnits> {
    chunks
        .iter()
        .map(|seg| SegmentUnits {
            segment: seg.label.clone(),
            scenes: extract_scenes(&seg.content),
        })
        .collect()
}

/// 组合探测（探测 + 切块都完成后调用；测试与诊断用）
#[allow(dead_code)]
pub fn probe(content: &str, chunks: &[SourceSegment]) -> ProbeResult {
    let (form, ratio) = probe_form(content);
    ProbeResult {
        form,
        attribution_hit_ratio: ratio,
        citation_units: citation_units(chunks),
    }
}

/// 段内场次标题提取：整行匹配场次模式且去重保序
fn extract_scenes(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in content.lines() {
        if chunk::is_scene_heading(line) {
            let t = line.trim().to_string();
            if !out.contains(&t) {
                out.push(t);
            }
        }
    }
    out
}

/// 台词的场次归属：文本中位置 line_idx 之前最近的场次标题；
/// 无场次标题时返回段 label
pub(crate) fn scene_at(segment: &str, seg_label: &str, line_idx: usize) -> String {
    let mut current = String::new();
    for (i, line) in segment.lines().enumerate() {
        if i >= line_idx {
            break;
        }
        if chunk::is_scene_heading(line) {
            current = line.trim().to_string();
        }
    }
    if current.is_empty() {
        seg_label.to_string()
    } else {
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script_sample() -> String {
        [
            "第1场 公安会议室 日",
            "玛奇玛：电次君，你的选项有两个。",
            "电次：什么选项？",
            "玛奇玛：作为恶魔被我杀掉，还是作为人被我饲养。",
            "（电次沉默）",
            "第2场 公园 夜",
            "电次：真的可以吗？",
            "玛奇玛：回复只需要「是」或「汪」。",
        ]
        .join("\n")
    }

    #[test]
    fn dialogue_line_parsing() {
        let (n, l) = parse_dialogue_line("玛奇玛：你的选项有两个。").unwrap();
        assert_eq!(n, "玛奇玛");
        assert_eq!(l, "你的选项有两个。");
        let (n, _) = parse_dialogue_line("【岸边】活下去。").unwrap();
        assert_eq!(n, "岸边");
        assert!(parse_dialogue_line("他说：这不是台词归属").is_none());
        assert!(parse_dialogue_line("普通叙述行").is_none());
    }

    #[test]
    fn probe_detects_script_form() {
        let text = script_sample();
        let chunks = chunk::build_chunks(&text);
        let p = probe(&text, &chunks.segments);
        assert_eq!(p.form, "script");
        assert!(p.attribution_hit_ratio > 0.3);
    }

    #[test]
    fn probe_detects_prose_form() {
        let text = "他走进房间，环顾四周。墙上挂着一幅画，画中是海。他想起很多年前的事，那时他还小。\n她低着头，没有说话。窗外的雨一直下，像是要把整个城市淹没。他们就这样站了很久。";
        let chunks = chunk::build_chunks(text);
        let p = probe(text, &chunks.segments);
        assert_eq!(p.form, "prose");
    }

    #[test]
    fn citation_units_extract_scenes() {
        let text = script_sample();
        let chunks = chunk::build_chunks(&text);
        let p = probe(&text, &chunks.segments);
        let units = &p.citation_units[0];
        assert_eq!(units.scenes.len(), 2);
        assert!(units.scenes[0].contains("第1场"));
    }

    #[test]
    fn scene_at_finds_nearest() {
        let seg = script_sample();
        // 第 5 行（0 起）属于第1场之后、第2场之前
        assert!(scene_at(&seg, "片段 1", 5).contains("第1场"));
        assert!(scene_at(&seg, "片段 1", 8).contains("第2场"));
        // 第 0 行之前无场次 → 段 label
        assert_eq!(scene_at(&seg, "片段 1", 0), "片段 1");
    }
}
