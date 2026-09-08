// 证据层：段笔记类型（LLM 产出）+ 证据包汇编（确定性为主）。
// 规范见 docs/工具实现/角色卡提炼如何实现.md §5.1 / §5.3。
// 台词不进摘要、不意译——引用素材必须原文。

use crate::deconstruct::chunk::SourceSegment;
use crate::deconstruct::probe;
use serde::{Deserialize, Serialize};

// ---- 段笔记（结构化输出契约，宽容解析：字段缺失给默认值） ----

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SegmentNote {
    /// 所属切块段 label（程序回填，不信任模型输出）
    #[serde(default)]
    pub segment: String,
    #[serde(default)]
    pub scene_summaries: Vec<SceneSummary>,
    /// 台上角色（原文写法，含别名，不归并）
    #[serde(default)]
    pub characters_on_stage: Vec<String>,
    #[serde(default)]
    pub events: Vec<NoteEvent>,
    #[serde(default)]
    pub relationship_signals: Vec<RelationshipSignal>,
    /// 幕后提及/评价
    #[serde(default)]
    pub mentions: Vec<Mention>,
    /// 段内对白（原文照录；script form 以确定性抽取为准，此处为补充证据）
    #[serde(default)]
    pub dialogues: Vec<NoteDialogue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SceneSummary {
    pub scene: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct NoteEvent {
    pub scene: String,
    pub what: String,
    #[serde(default)]
    pub who: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emotion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RelationshipSignal {
    pub a: String,
    pub b: String,
    pub signal: String,
    pub scene: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Mention {
    pub about: String,
    pub by: String,
    pub content: String,
    pub scene: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct NoteDialogue {
    pub character: String,
    pub line: String,
    pub scene: String,
}

// ---- 证据包 ----

/// 台词语料上限与 push 上限（规范 §6 常量）
const DIALOGUE_CORPUS_MAX_LINES: usize = 2000;
const DIALOGUE_PUSH_MAX_LINES: usize = 400;
const DIALOGUE_EDGE_KEEP: usize = 40;
/// 别名参与包含匹配的最短长度（防单字误伤）
const ALIAS_MIN_LEN_FOR_CONTAINS: usize = 2;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Dialogue {
    pub scene: String,
    pub line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ActionLine {
    pub scene: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EvidencePack {
    pub character: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// 台词全录（或采样后）
    pub dialogues: Vec<Dialogue>,
    /// 角色邻近的动作/舞台指示行（原始素材，未过滤）
    #[serde(default)]
    pub actions: Vec<ActionLine>,
    /// 时间序事件（来自各段笔记）
    #[serde(default)]
    pub events: Vec<NoteEvent>,
    #[serde(default)]
    pub mentions: Vec<Mention>,
    #[serde(default)]
    pub relationship_signals: Vec<RelationshipSignal>,
    /// dialogues 是否已采样
    pub sampled: bool,
}

/// 名字匹配：目标串 == 名字或别名（精确），或目标串包含名字/别名（长度 ≥ 2 防单字误伤）。
/// 用于台词归属（说话人短名）与笔记角色字段（who/about/a/b 可能带修饰）。
pub fn name_matches(name: &str, aliases: &[String], target: &str) -> bool {
    let t = target.trim();
    if t.is_empty() {
        return false;
    }
    let name = name.trim();
    if t == name {
        return true;
    }
    let contains = |key: &str| {
        key.chars().count() >= ALIAS_MIN_LEN_FOR_CONTAINS && t.contains(key)
    };
    contains(name) || aliases.iter().any(|a| contains(a.trim()))
}

/// 证据包汇编。script form：台词按归属模式确定性抽取；prose form：台词取自各段笔记 dialogues。
pub fn build_pack(
    name: &str,
    aliases: &[String],
    segments: &[SourceSegment],
    notes: &[SegmentNote],
    form: &str,
) -> EvidencePack {
    let mut pack = EvidencePack {
        character: name.to_string(),
        aliases: aliases.to_vec(),
        ..Default::default()
    };

    if form == "script" {
        for seg in segments {
            let lines: Vec<&str> = seg.content.lines().collect();
            let mut spoken_at: Vec<usize> = Vec::new();
            for (i, line) in lines.iter().enumerate() {
                if let Some((speaker, speech)) = probe::parse_dialogue_line(line)
                    && name_matches(name, aliases, &speaker) && !speech.trim().is_empty() {
                        pack.dialogues.push(Dialogue {
                            scene: probe::scene_at(&seg.content, &seg.label, i),
                            line: speech,
                        });
                        spoken_at.push(i);
                    }
            }
            // 动作行：该角色台词行 ±2 行内的非台词行（含括号舞台指示或含角色名）
            for (i, line) in lines.iter().enumerate() {
                if !spoken_at.is_empty()
                    && probe::parse_dialogue_line(line).is_none()
                    && !crate::deconstruct::chunk::is_scene_heading(line)
                    && !line.trim().is_empty()
                {
                    let near = spoken_at.iter().any(|&s| i.abs_diff(s) <= 2);
                    let paren = line.trim_start().starts_with('（') || line.trim_start().starts_with('(');
                    if near && (paren || line.contains(name) || aliases.iter().any(|a| line.contains(a.trim()))) {
                        pack.actions.push(ActionLine {
                            scene: probe::scene_at(&seg.content, &seg.label, i),
                            text: line.trim().to_string(),
                        });
                    }
                }
            }
        }
    }

    // 笔记聚合（两种 form 都做：事件/提及/关系信号是 soul 与关系文件的主证据）
    for note in notes {
        for ev in &note.events {
            if ev.who.iter().any(|w| name_matches(name, aliases, w)) {
                pack.events.push(ev.clone());
            }
        }
        for m in &note.mentions {
            if name_matches(name, aliases, &m.about) {
                pack.mentions.push(m.clone());
            }
        }
        for rs in &note.relationship_signals {
            if name_matches(name, aliases, &rs.a) || name_matches(name, aliases, &rs.b) {
                pack.relationship_signals.push(rs.clone());
            }
        }
        // prose form 台词来自笔记
        if form != "script" {
            for d in &note.dialogues {
                if name_matches(name, aliases, &d.character) {
                    pack.dialogues.push(Dialogue {
                        scene: d.scene.clone(),
                        line: d.line.clone(),
                    });
                }
            }
        }
    }

    // 采样：> 2000 条 → 等距采样至 400（保留首尾各 40），全量语料不在此保留（任务目录有原文）
    if pack.dialogues.len() > DIALOGUE_CORPUS_MAX_LINES {
        pack.dialogues = sample_evenly(&pack.dialogues, DIALOGUE_PUSH_MAX_LINES, DIALOGUE_EDGE_KEEP);
        pack.sampled = true;
    }
    pack
}

/// 等距采样（确定性）：保留首尾各 edge 条，中间等距取 total-2*edge 条
fn sample_evenly(items: &[Dialogue], total: usize, edge: usize) -> Vec<Dialogue> {
    let n = items.len();
    if n <= total || total <= 2 * edge {
        return items.to_vec();
    }
    let mut out: Vec<Dialogue> = Vec::with_capacity(total);
    out.extend_from_slice(&items[..edge]);
    let middle = &items[edge..n - edge];
    let take = total - 2 * edge;
    for k in 0..take {
        let idx = (k * middle.len()) / take;
        out.push(middle[idx].clone());
    }
    out.extend_from_slice(&items[n - edge..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg() -> SourceSegment {
        SourceSegment {
            label: "第1场 ~ 第2场".into(),
            content: [
                "第1场 会议室 日",
                "玛奇玛：电次君，你的选项有两个。",
                "（电次抬头看她）",
                "电次：什么选项？",
                "岸边：别上当。",
                "玛奇玛：作为恶魔被我杀掉，还是作为人被我饲养。",
                "第2场 公园 夜",
                "玛奇玛：回复只需要「是」或「汪」。",
            ]
            .join("\n"),
        }
    }

    #[test]
    fn name_matching() {
        assert!(name_matches("电次", &["电次君".into()], "电次"));
        assert!(name_matches("电次", &["电次君".into()], "电次君"));
        assert!(name_matches("电次", &["电次君".into()], "老年电次"));
        assert!(name_matches("电次", &[], "电次君"), "目标包含名字（≥2 字）也应命中");
        assert!(!name_matches("电", &[], "这次不一样"), "单字不参与包含匹配");
        assert!(!name_matches("电次", &["次".into()], "这次不一样")); // 单字别名不参与包含
    }

    #[test]
    fn pack_script_dialogues_and_scenes() {
        let s = seg();
        let pack = build_pack("玛奇玛", &[], &[s.clone()], &[], "script");
        assert_eq!(pack.dialogues.len(), 3);
        assert_eq!(pack.dialogues[0].line, "电次君，你的选项有两个。");
        assert!(pack.dialogues[0].scene.contains("第1场"));
        assert!(pack.dialogues[2].scene.contains("第2场"));
        // 动作行：玛奇玛台词邻近的「（电次抬头看她）」
        assert!(pack.actions.iter().any(|a| a.text.contains("电次抬头")));
    }

    #[test]
    fn pack_aggregates_notes() {
        let note = SegmentNote {
            segment: "片段 1".into(),
            scene_summaries: vec![],
            characters_on_stage: vec!["玛奇玛".into(), "电次".into()],
            events: vec![NoteEvent {
                scene: "第1场".into(),
                what: "提出饲养选项".into(),
                who: vec!["玛奇玛".into(), "电次".into()],
                emotion: Some("平静".into()),
            }],
            relationship_signals: vec![RelationshipSignal {
                a: "玛奇玛".into(),
                b: "电次".into(),
                signal: "支配".into(),
                scene: "第1场".into(),
            }],
            mentions: vec![Mention {
                about: "玛奇玛".into(),
                by: "岸边".into(),
                content: "她是恶魔".into(),
                scene: "第1场".into(),
            }],
            dialogues: vec![],
        };
        let pack = build_pack("玛奇玛", &[], &[], &[note], "prose");
        assert_eq!(pack.events.len(), 1);
        assert_eq!(pack.mentions.len(), 1);
        assert_eq!(pack.relationship_signals.len(), 1);
    }

    #[test]
    fn sampling_deterministic_and_bounded() {
        let items: Vec<Dialogue> = (0..3000)
            .map(|i| Dialogue {
                scene: format!("s{}", i / 100),
                line: format!("台词{}", i),
            })
            .collect();
        let sampled = sample_evenly(&items, 400, 40);
        assert_eq!(sampled.len(), 400);
        assert_eq!(sampled[0].line, "台词0");
        assert_eq!(sampled.last().unwrap().line, "台词2999");
        // 确定性
        assert_eq!(sampled, sample_evenly(&items, 400, 40));
        // 不超限不采样
        let small: Vec<Dialogue> = items[..100].to_vec();
        assert_eq!(sample_evenly(&small, 400, 40).len(), 100);
    }
}
