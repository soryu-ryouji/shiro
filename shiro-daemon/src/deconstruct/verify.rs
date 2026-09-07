// 引文回查（quote-verify）：确定性、零 LLM。
// 解析卡内全部「……」（引用单元）→ 归一化 → 定位匹配（V1 简化回退：引用单元文本 → 全文）；
// 失败 → 打回重生成一次 → 二次失败删引文并记录报告（标黄交人工）。规范 §5.5。

use crate::deconstruct::chunk::SourceSegment;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct QuoteCite {
    pub quote: String,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, utoipa::ToSchema)]
pub struct VerifyReport {
    /// 回查通过的引文数
    pub passed: usize,
    /// 二次失败后被删除的引文（file, quote, unit）
    pub removed: Vec<RemovedQuote>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct RemovedQuote {
    pub file: String,
    pub quote: String,
    pub unit: String,
}

/// 「引文」（引用单元）
static QUOTE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"「([^」]{2,300})」（([^）]{1,80})）"#).unwrap());

/// 提取文本中全部引文与引用单元
pub fn extract_quotes(text: &str) -> Vec<QuoteCite> {
    QUOTE_RE
        .captures_iter(text)
        .filter_map(|c| {
            let quote = c.get(1)?.as_str().to_string();
            let unit = c.get(2)?.as_str().trim().to_string();
            Some(QuoteCite { quote, unit })
        })
        .collect()
}

/// 归一化：去全部空白；去引号类字符（原文台词常无「」，模型引用时加了引号）；
/// 统一常见全半角标点（冒号/分号/逗号/句号/问号/叹号）
pub fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '「' | '」' | '『' | '』' | '"' | '\'' | '“' | '”' | '‘' | '’'))
        .map(|c| match c {
            '：' | '﹕' => ':',
            '，' | '､' => ',',
            '。' => '.',
            '？' => '?',
            '！' => '!',
            '；' => ';',
            '（' => '(',
            '）' => ')',
            '…' => '.',
            '—' | '―' | '–' => '-',
            other => other,
        })
        .collect()
}

/// 单个文件全部引文的回查：返回未命中的引文列表（去重）
pub fn verify_text(text: &str, segments: &[SourceSegment], full_text: &str) -> Vec<QuoteCite> {
    let full_norm = normalize(full_text);
    let seg_norms: Vec<(String, String)> = segments
        .iter()
        .map(|s| (s.label.clone(), normalize(&s.content)))
        .collect();
    let mut failed = Vec::new();
    for qc in extract_quotes(text) {
        let q = normalize(&qc.quote);
        if q.is_empty() {
            continue;
        }
        let hit = if qc.unit == "SEG" {
            // 未回填的引用单元：直接全文匹配
            full_norm.contains(&q)
        } else {
            // 引用单元内匹配优先；否则全文（V1 简化：跳过相邻单元回退）
            let unit_hit = seg_norms
                .iter()
                .any(|(label, body)| label.contains(&qc.unit) && body.contains(&q));
            unit_hit || full_norm.contains(&q)
        };
        if !hit && !failed.contains(&qc) {
            failed.push(qc);
        }
    }
    failed
}

/// 删除失败引文的「」标记（保留内部文字，机器可识别的降级标记），返回修改后的文本
pub fn strip_failed_quotes(text: &str, failed: &[QuoteCite]) -> String {
    let mut out = text.to_string();
    for qc in failed {
        let marked = format!("「{}」（{}）", qc.quote, qc.unit);
        let degraded = format!("{}（引文未在原文定位，已去除引号）", qc.quote);
        out = out.replace(&marked, &degraded);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segs() -> Vec<SourceSegment> {
        vec![SourceSegment {
            label: "第1场 会议室".into(),
            content: "第1场 会议室\n玛奇玛：你的选项有两个。\n电次：什么选项？".into(),
        }]
    }

    #[test]
    fn extract_and_verify() {
        let text = "她说过「你的选项有两个。」（第1场 会议室），这是关键。";
        let qcs = extract_quotes(text);
        assert_eq!(qcs.len(), 1);
        assert_eq!(qcs[0].quote, "你的选项有两个。");
        assert_eq!(qcs[0].unit, "第1场 会议室");

        let failed = verify_text(text, &segs(), "全文任意位置");
        assert!(failed.is_empty(), "段内命中");
    }

    #[test]
    fn fulltext_fallback_and_quote_normalization() {
        // 引用单元不存在，但引文在全文 → 通过（全文回退）
        let text = "结论：「什么选项？」（第X场 不存在的场）。";
        let failed = verify_text(text, &segs(), "前文随便\n电次：什么选项？\n后文");
        assert!(failed.is_empty());

        // 引文被改写（多了"请"字）→ 失败
        let text2 = "结论：「请说说选项？」（第1场 会议室）。";
        let failed2 = verify_text(text2, &segs(), "无关文本");
        assert_eq!(failed2.len(), 1);
    }

    #[test]
    fn normalization_strips_quotes_and_space() {
        // 模型引文带引号/空白，原文台词无引号 → 归一化后命中
        let text = "她说「 电次， “活下去” 」（第1场）。";
        let segs2 = vec![SourceSegment {
            label: "第1场".into(),
            content: "第1场\n玛奇玛：电次，“活下去”".into(),
        }];
        assert!(verify_text(text, &segs2, "").is_empty());
    }

    #[test]
    fn strip_marks_degraded() {
        let text = "原句「被改写的引文」（第1场）保留在论断里。";
        let failed = vec![QuoteCite {
            quote: "被改写的引文".into(),
            unit: "第1场".into(),
        }];
        let out = strip_failed_quotes(text, &failed);
        assert!(!out.contains('「'));
        assert!(out.contains("引文未在原文定位"));
    }
}
