// 提示词模板（拆解节点组共用约束）：段笔记 + 七文件生成。
// 输出约束统一钉在 system：论断绑证据、引文原文照录、留白不猜测、引用单元格式可机读。
// 质量基线参考 docs/角色卡模板/玛奇玛角色卡.md（论断鲜明 + 全部可回溯，不是模板填空）。

use crate::deconstruct::evidence::{EvidencePack, SegmentNote};

/// 引用单元占位符：模型无从得知程序侧段 label，统一填 SEG，程序在解析后回填真实段 label
pub(crate) const SEG_PLACEHOLDER: &str = "SEG";

const COMMON_SYSTEM: &str = r#"你是剧本角色研究员，为外部剧本建立角色写作档案（还原型：保留原作专名与引文）。

铁律：
1. 论断必须钉在证据上：每个重要性格/行为论断至少绑定一条引文或引用单元；材料无法支撑的性格词不许写
2. 引文格式固定为「……」（引用单元），机器要回查原文，因此引文必须逐字照录，禁止改写、缩写、凭印象补写
3. 引用单元：优先用场次/章节标题原文；看不出场次结构时填 SEG
4. 原作未交代的留白，如实标注「留白」，不得推测填充
5. 只依据给定材料，不引入材料之外的常识或同人设定"#;

fn system(work: &str, character: &str) -> String {
    format!("{COMMON_SYSTEM}\n\n当前作品：《{work}》；研究对象：{character}")
}

/// 段笔记（map 节点）。输出 JSON，结构见提示词。
pub(crate) fn notes_prompt(seg_label: &str, content: &str) -> (String, String) {
    let system = "你是剧本结构研究员。阅读给定片段，输出结构化 JSON 笔记。只记录片段内出现的事实，不推测片段之外的内容。台词一律原文照录，禁止概括改写。".to_string();
    let user = format!(
        r#"【剧本片段：{seg_label}】
{content}

输出 JSON（只输出 JSON，不要任何其他文字）：
{{
  "scene_summaries": [{{"scene": "场次标题（无场次结构填 SEG）", "summary": "本单元发生了什么，1-2 句"}}],
  "characters_on_stage": ["本片段出现的角色，按原文写法"],
  "events": [{{"scene": "…", "what": "发生了什么动作/转折", "who": ["参与者"], "emotion": "参与者显性情绪，无则省略该字段"}}],
  "relationship_signals": [{{"a": "角色A", "b": "角色B", "signal": "两人间的态度/权力/情感信号", "scene": "…"}}],
  "mentions": [{{"about": "被谈论的角色", "by": "谈论者", "content": "谈论内容（直接引语原文照录）", "scene": "…"}}],
  "dialogues": [{{"character": "说话者", "line": "台词原文", "scene": "…"}}]
}}

要求：
- dialogues 收录本片段全部台词（若片段超长，保留与主要角色相关的台词，每条原文照录）
- events 记录动作与转折，不只是对话内容
- mentions 是幕后角色的关键证据：角色不在场时被如何评价
- 没有内容的字段给空数组"#
    );
    (system, user)
}

/// 笔记的引用单元回填：scene 为空或 SEG 时替换为段 label
pub(crate) fn backfill_segment(note: &mut SegmentNote, seg_label: &str) {
    let fix = |s: &mut String| {
        let t = s.trim();
        if t.is_empty() || t == SEG_PLACEHOLDER {
            *s = seg_label.to_string();
        }
    };
    note.segment = seg_label.to_string();
    for s in &mut note.scene_summaries {
        fix(&mut s.scene);
    }
    for e in &mut note.events {
        fix(&mut e.scene);
    }
    for r in &mut note.relationship_signals {
        fix(&mut r.scene);
    }
    for m in &mut note.mentions {
        fix(&mut m.scene);
    }
    for d in &mut note.dialogues {
        fix(&mut d.scene);
    }
}

fn dialogue_block(pack: &EvidencePack, limit: usize) -> String {
    let lines: Vec<String> = pack
        .dialogues
        .iter()
        .take(limit)
        .map(|d| format!("[{}] {}", d.scene, d.line))
        .collect();
    let note = if pack.sampled {
        format!("（台词已等距采样，全量 {} 条）", pack.dialogues.len())
    } else {
        String::new()
    };
    lines.join("\n") + &note
}

fn mention_block(pack: &EvidencePack) -> String {
    if pack.mentions.is_empty() {
        return "（无他人评价材料）".into();
    }
    pack.mentions
        .iter()
        .map(|m| format!("[{}] {}（{}）：{}", m.scene, m.by, m.about, m.content))
        .collect::<Vec<_>>()
        .join("\n")
}

fn event_block(pack: &EvidencePack) -> String {
    if pack.events.is_empty() {
        return "（无事件记录）".into();
    }
    pack.events
        .iter()
        .map(|e| {
            let who = e.who.join("、");
            match &e.emotion {
                Some(em) if !em.trim().is_empty() => format!("[{}] {}（{}，情绪：{}）", e.scene, e.what, who, em),
                _ => format!("[{}] {}（{}）", e.scene, e.what, who),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn relation_block(pack: &EvidencePack) -> String {
    if pack.relationship_signals.is_empty() {
        return "（无关系信号）".into();
    }
    pack.relationship_signals
        .iter()
        .map(|r| format!("[{}] {} ↔ {}：{}", r.scene, r.a, r.b, r.signal))
        .collect::<Vec<_>>()
        .join("\n")
}

fn action_block(pack: &EvidencePack) -> String {
    if pack.actions.is_empty() {
        return "（无动作/舞台指示摘录）".into();
    }
    pack.actions
        .iter()
        .map(|a| format!("[{}] {}", a.scene, a.text))
        .collect::<Vec<_>>()
        .join("\n")
}

fn synopsis_block(notes: &[SegmentNote]) -> String {
    let mut out = Vec::new();
    for n in notes {
        if n.scene_summaries.is_empty() {
            continue;
        }
        let scenes: Vec<String> = n
            .scene_summaries
            .iter()
            .map(|s| format!("{}：{}", s.scene, s.summary))
            .collect();
        out.push(format!("【{}】\n{}", n.segment, scenes.join("\n")));
    }
    if out.is_empty() {
        return "（无场次摘要）".into();
    }
    out.join("\n\n")
}

/// soul.md：核心驱动力、价值观、根本矛盾、弧线判定、情绪处理机制（生成顺序第 1）
pub(crate) fn soul_prompt(work: &str, character: &str, pack: &EvidencePack, notes: &[SegmentNote]) -> (String, String) {
    let user = format!(
        r#"材料一：作品梗概（各片段场次摘要合成）
{synopsis}

材料二：{name} 的事件时间线（含情绪注记）
{events}

材料三：他人对 {name} 的评价（幕后提及）
{mentions}

任务：写 soul.md（markdown，不要 frontmatter），结构如下：

## 核心驱动力
（她/他要什么，排在最前面的东西；用证据说话）

## 价值观
（什么会被认可/嘲讽/无视；与常人差异处）

## 根本矛盾
（2-4 条，每条都是无法自我调和的对立）

## 弧线判定
（在「成长 / 揭示 / 堕落 / 无弧线」中明确判定一种，给出依据；这是后续所有档案的锚点，判定后不得在其他文件中写出与弧线矛盾的变化）

## 情绪处理机制
（默认状态；压力应对；情绪如何泄露进台词与动作——例如措辞如何变化、动作如何变化，附引文）"#,
        synopsis = synopsis_block(notes),
        events = event_block(pack),
        mentions = mention_block(pack),
        name = character,
    );
    (system(work, character), user)
}

/// speech_patterns.md：句式指纹、称谓、口癖、跨对象差异、标志性台词（第 2）
pub(crate) fn speech_prompt(work: &str, character: &str, pack: &EvidencePack, soul: &str) -> (String, String) {
    let user = format!(
        r#"材料一：{name} 的全部台词语料（按时序，[引用单元] 台词）
{dialogues}

材料二：soul.md 结论（语言必须与灵魂一致）
{soul}

任务：写 speech_patterns.md（markdown，不要 frontmatter），结构如下：

## 言语基调
（语速、语体、温度；内容与语调的反差规律）

## 句式指纹
（祈使/反问/省略/选择题……每种句式附至少一条原文引文）

## 称谓系统
（怎么称呼每个人；称谓是否恒定；称谓变化的信号意义）

## 口癖与高频表达
（高频词句与出现密度估计；注意：高频不等于口头禅，不要硬凑）

## 跨对象语言差异
（对上/对下/对敌/对亲近者的语域切换，各附引文对照）

## 标志性台词
（5-10 条，逐字照录并附引用单元；宁缺毋滥，无法逐字定位的不许列）"#,
        name = character,
        dialogues = dialogue_block(pack, 800),
    );
    (system(work, character), user)
}

/// behavior_guide.md：动作库、社交剧本、If-Then 规则（第 3）
pub(crate) fn behavior_prompt(work: &str, character: &str, pack: &EvidencePack, soul: &str) -> (String, String) {
    let user = format!(
        r#"材料一：{name} 的动作/舞台指示摘录（原始素材）
{actions}

材料二：事件时间线
{events}

材料三：soul.md 结论（行为必须与灵魂一致）
{soul}

任务：写 behavior_guide.md（markdown，不要 frontmatter），结构如下：

## 动作库
（高频动作/姿态/习惯，每条附出处；区分「她做的」与「她让别人做的」）

## 社交剧本
（面对请求/威胁/示好/质问/背叛时的标准应对策略，各附证据）

## If-Then 行为规则
（遇 X 则 Y，至少 8 条，每条都要有证据支撑或与 soul 结论直接一致；这是写新场景时的行为查表）

## 冲突协议
（如适用：她如何战斗/报复/清除障碍——直接动手还是借力，附证据）"#,
        name = character,
        actions = action_block(pack),
        events = event_block(pack),
    );
    (system(work, character), user)
}

/// relationship_dynamics.md：逐关系拆解（第 4）
pub(crate) fn relationship_prompt(work: &str, character: &str, pack: &EvidencePack, soul: &str) -> (String, String) {
    let user = format!(
        r#"材料一：{name} 的关系信号全集
{relations}

材料二：他人对 {name} 的评价
{mentions}

材料三：soul.md 结论（关系的性质必须与灵魂一致——如果 soul 判定她只会建立支配关系，就不要写她对等关系）
{soul}

任务：写 relationship_dynamics.md（markdown，不要 frontmatter）：
- 按关系对象分节（## 对象名），只写材料中出现过实质互动的对象（3-6 个主要对象）
- 每节含：关系性质、权力方向（谁在上的具体表现）、她对这个人的策略、关键场景证据（引文）
- 最后一节 ## 关系总模式（所有关系共同的结构规律）"#,
        name = character,
        relations = relation_block(pack),
        mentions = mention_block(pack),
        soul = soul,
    );
    (system(work, character), user)
}

/// key_life_events.md：编年事件 + 留白区清单（第 5）
pub(crate) fn events_prompt(work: &str, character: &str, pack: &EvidencePack, notes: &[SegmentNote], soul: &str) -> (String, String) {
    let user = format!(
        r#"材料一：作品梗概
{synopsis}

材料二：涉及 {name} 的事件时间线
{events}

材料三：soul.md 结论
{soul}

任务：写 key_life_events.md（markdown，不要 frontmatter），结构如下：

## 编年事件锚点
（按时间序列出关键事件：每个事件一行，附引用单元；只写材料中有的）

## 留白区清单
（原作未交代的重要信息：身世细节、动机深处、关键时间段空白……逐条列出，每条标注「写作时不得填充」的边界）"#,
        synopsis = synopsis_block(notes),
        events = event_block(pack),
        name = character,
    );
    (system(work, character), user)
}

/// limit.md：OOC 红线、证据规则（第 6；兼作后续生成工作流的写作约束）
pub(crate) fn limit_prompt(work: &str, character: &str, files: &[( &str, &str)]) -> (String, String) {
    let conclusions: String = files
        .iter()
        .map(|(name, body)| format!("## 来自 {name}\n{}\n", truncate_chars(body, 2500)))
        .collect::<Vec<_>>()
        .join("\n");
    let user = format!(
        r#"以下是 {name} 档案前五份文件的结论材料：

{conclusions}

任务：写 limit.md（markdown，不要 frontmatter），结构如下：

## OOC 红线
（写了就崩：与灵魂/弧线/语言直接矛盾的行为与台词模式，逐条列出「禁止 X，因为 Y」）

## 证据规则
（哪些论断有原文支撑、哪些是合理推断、哪些是留白——写作时的引用纪律）

## 卡内禁写事项
（与其他档案结论冲突的常见写法，逐条禁止）"#,
        name = character,
    );
    (system(work, character), user)
}

/// index.md 正文（第 7，最后生成：对全部子文件的压缩提炼，不是填空）
pub(crate) fn index_prompt(work: &str, character: &str, files: &[(&str, &str)]) -> (String, String) {
    let bundle: String = files
        .iter()
        .map(|(name, body)| format!("───── {name} ─────\n{body}\n"))
        .collect::<Vec<_>>()
        .join("\n");
    let user = format!(
        r#"以下是 {name} 档案的全部子文件：

{bundle}

任务：写 index.md 的正文（markdown，不要 frontmatter，程序会自动加 frontmatter），结构如下：

## 一句话抓住这个角色
（一句凝练的判词，必须能统摄下方全部文件的核心结论）

## 三条铁律
（写这个角色时不可违背的三条；每条一句话，加粗关键词）

## 阅读顺序
（写哪种场景先查哪个文件，逐条指引）

## 使用注意
（3-5 条军规：最容易写崩的地方怎么防）"#,
        name = character,
    );
    (system(work, character), user)
}

/// 引文回查失败的修复提示（verify 打回重生成用）
pub(crate) fn regen_with_failed_quotes(base_user: &str, failed: &[String]) -> String {
    let list = failed
        .iter()
        .map(|q| format!("- 「{q}」"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "{base_user}\n\n【必须修复】以下引文未能在原文中定位（可能被改写或凭印象补写），请逐字核对材料修正；确实无法定位的整条删除，不要保留改写版：\n{list}"
    )
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "\n…（已截断）"
    }
}
