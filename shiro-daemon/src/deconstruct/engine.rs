// 拆解任务引擎：状态机 + 后台执行 + 进度落盘（~/.config/shiro/db/deconstruct/<id>/）。
// 失败语义（规范 §7）：丢正文类阻断；质量类降级显式标注。每步进度原子写 task.json。

use crate::api::now_secs;
use crate::deconstruct::chunk::{self, ChunkResult};
use crate::deconstruct::evidence::{self, EvidencePack, SegmentNote};
use crate::deconstruct::prompts;
use crate::deconstruct::probe::{self, ProbeResult};
use crate::deconstruct::verify::{self, RemovedQuote, VerifyReport};
use crate::llm::{self, ChatMessage, LlmConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// 段笔记解析失败重试上限（规范 §6）
const NOTES_MAX_RETRY: usize = 2;
/// 引文回查失败后打回重生成上限
const QUOTE_REGEN_MAX: usize = 1;

// ---- 状态与进度 ----

pub const STAGE_PENDING: &str = "pending";
pub const STAGE_PROBING: &str = "probing";
pub const STAGE_CHUNKING: &str = "chunking";
pub const STAGE_NOTES: &str = "notes";
pub const STAGE_EVIDENCE: &str = "evidence";
pub const STAGE_GENERATING: &str = "generating";
pub const STAGE_VERIFYING: &str = "verifying";
pub const STAGE_DONE: &str = "done";
pub const STAGE_FAILED: &str = "failed";
/// 切块完成后的用户选择闸门：等待用户挑选段落（非运行态，续跑不自动恢复）
pub const STAGE_SELECTING: &str = "selecting";

pub const GENERATION_FILES: [&str; 7] = [
    "soul",
    "speech_patterns",
    "behavior_guide",
    "relationship_dynamics",
    "key_life_events",
    "limit",
    "index",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskMeta {
    pub id: String,
    /// 导入的素材名（作品名 / 文件名）
    pub source_name: String,
    pub character: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, utoipa::ToSchema)]
pub struct Progress {
    pub stage: String,
    /// 素材形态（script/prose；探测后回填）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<String>,
    #[serde(default)]
    pub segment_count: usize,
    #[serde(default)]
    pub notes_done: usize,
    /// 解析失败的段序号（降级，不阻断）
    #[serde(default)]
    pub notes_failed: Vec<usize>,
    /// 生成中的当前文件（不带扩展名）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_file: Option<String>,
    #[serde(default)]
    pub files_done: Vec<String>,
    /// 用户选中的段序号（选择闸门回填；空 = 尚未选择）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<Vec<usize>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified: Option<VerifyReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_character_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
}

impl Progress {
    pub fn is_terminal(&self) -> bool {
        self.stage == STAGE_DONE || self.stage == STAGE_FAILED
    }
    /// 运行态：非终态且不在选择闸门（selecting 是人工暂停点，重启不自动续跑）
    pub fn is_running(&self) -> bool {
        !self.stage.is_empty() && !self.is_terminal() && self.stage != STAGE_SELECTING
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskRecord {
    pub meta: TaskMeta,
    pub progress: Progress,
}

// ---- 任务目录布局 ----
// <root>/<id>/task.json（元数据+进度快照） source.txt chunks.json probe.json
//            notes/<i>.json evidence.json card/<file>.md report.json

pub fn tasks_root() -> PathBuf {
    crate::api::config_dir().join("db").join("deconstruct")
}

fn task_dir(id: &str) -> Option<PathBuf> {
    valid_task_id(id).then(|| tasks_root().join(id))
}

fn valid_task_id(id: &str) -> bool {
    !id.is_empty() && !id.starts_with('.') && !id.contains(['/', '\\'])
}

/// 原子写（临时文件 + rename）
fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)
}

fn read_record(id: &str) -> Option<TaskRecord> {
    let dir = task_dir(id)?;
    let text = std::fs::read_to_string(dir.join("task.json")).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_record(record: &TaskRecord) -> std::io::Result<()> {
    let dir = task_dir(&record.meta.id).expect("id 已校验");
    let json = serde_json::to_string_pretty(record).map_err(std::io::Error::other)?;
    atomic_write(&dir.join("task.json"), &json)
}

/// 中断检查：磁盘上非终态但不在内存表 → daemon 重启中断（返回修改后的记录）
fn check_interrupted(rec: &mut TaskRecord, running: bool) -> bool {
    if rec.progress.is_running() && !running {
        rec.progress.stage = STAGE_FAILED.into();
        rec.progress.error = Some("任务中断（daemon 重启）。可删除后重新制作".into());
        rec.progress.finished_at = Some(now_secs());
        true
    } else {
        false
    }
}

// ---- Hub：内存中的运行任务表 ----

#[derive(Default)]
pub struct Hub {
    running: Mutex<HashMap<String, Arc<Mutex<Progress>>>>,
}

impl Hub {
    fn register(&self, id: &str, progress: Progress) -> Arc<Mutex<Progress>> {
        let handle = Arc::new(Mutex::new(progress));
        self.running
            .lock()
            .unwrap()
            .insert(id.to_string(), handle.clone());
        handle
    }

    fn unregister(&self, id: &str) {
        self.running.lock().unwrap().remove(id);
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.running.lock().unwrap().contains_key(id)
    }

    /// 任务列表（磁盘为准，最新在前）
    pub fn list_tasks(&self) -> Vec<TaskRecord> {
        let Ok(entries) = std::fs::read_dir(tasks_root()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let Some(id) = entry.file_name().to_str().map(String::from) else {
                continue;
            };
            let Some(mut rec) = read_record(&id) else {
                continue;
            };
            if check_interrupted(&mut rec, self.is_running(&id)) {
                let _ = write_record(&rec);
            }
            out.push(rec);
        }
        out.sort_by(|a, b| b.meta.created_at.cmp(&a.meta.created_at));
        out
    }

    pub fn get_task(&self, id: &str) -> Option<TaskRecord> {
        let mut rec = read_record(id)?;
        if check_interrupted(&mut rec, self.is_running(id)) {
            let _ = write_record(&rec);
        }
        Some(rec)
    }
}

// ---- 选择闸门：段清单（供用户挑选） ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SegmentInfo {
    /// 段在切块结果中的序号
    pub index: usize,
    pub label: String,
    /// 段字符数
    pub chars: usize,
    /// 开头预览（空白归一，前 80 字）
    pub excerpt: String,
    /// 段内是否出现目标角色（名字或别名，含包含匹配）——选择闸门高亮用
    #[serde(default)]
    pub has_name: bool,
}

fn seg_excerpt(content: &str) -> String {
    let text: String = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = text.chars().take(80).collect();
    if text.chars().count() > 80 {
        out.push('…');
    }
    out
}

/// 读取段清单（选择闸门已写盘时返回）
pub fn read_segments(id: &str) -> Vec<SegmentInfo> {
    task_dir(id)
        .and_then(|d| std::fs::read_to_string(d.join("segments.json")).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// 读取任务原文与元数据（复制为新制作的表单回填用）
pub fn read_source(id: &str) -> Option<(TaskMeta, String)> {
    let rec = read_record(id)?;
    let dir = task_dir(id)?;
    let content = std::fs::read_to_string(dir.join("source.txt")).ok()?;
    Some((rec.meta, content))
}

/// 用户提交段选择：校验后落盘并从选择闸门续跑
pub fn select_segments(
    hub: &Arc<Hub>,
    id: &str,
    selected: Vec<usize>,
) -> Result<TaskRecord, String> {
    if hub.is_running(id) {
        return Err("任务运行中，请稍候".into());
    }
    let mut rec = read_record(id).ok_or("任务不存在")?;
    if rec.progress.stage != STAGE_SELECTING {
        return Err("任务不在待选择状态".into());
    }
    let selected: Vec<usize> = selected
        .into_iter()
        .filter(|i| *i < rec.progress.segment_count)
        .collect();
    let mut selected = selected;
    selected.sort_unstable();
    selected.dedup();
    if selected.is_empty() {
        return Err("至少选择一段".into());
    }
    rec.progress.selected = Some(selected);
    rec.progress.stage = STAGE_PENDING.into();
    write_record(&rec).map_err(|e| e.to_string())?;
    let handle = hub.register(id, rec.progress.clone());
    let meta = rec.meta.clone();
    let h = hub.clone();
    tokio::spawn(async move {
        run_task(&h, meta, handle).await;
    });
    Ok(rec)
}

// ---- 创建与删除 ----

pub struct CreateParams {
    pub source_name: String,
    pub content: String,
    pub character: String,
    pub aliases: Vec<String>,
}

pub fn create_task(hub: &Arc<Hub>, params: CreateParams) -> Result<TaskRecord, String> {
    if params.content.trim().is_empty() {
        return Err("剧本内容为空".into());
    }
    if params.character.trim().is_empty() {
        return Err("角色名不能为空".into());
    }
    let character = params.character.trim().to_string();
    let id = format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let meta = TaskMeta {
        id: id.clone(),
        source_name: if params.source_name.trim().is_empty() {
            "未命名素材".into()
        } else {
            params.source_name.trim().to_string()
        },
        character: character.clone(),
        aliases: params
            .aliases
            .into_iter()
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty() && a != &character)
            .collect(),
        created_at: now_secs(),
    };
    let record = TaskRecord {
        meta: meta.clone(),
        progress: Progress {
            stage: STAGE_PENDING.into(),
            ..Default::default()
        },
    };
    let dir = task_dir(&id).expect("id 已生成");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("source.txt"), &params.content).map_err(|e| e.to_string())?;
    write_record(&record).map_err(|e| e.to_string())?;

    // 同步注册后再 spawn：关闭「磁盘已写 pending、内存未注册」的竞态窗口
    let handle = hub.register(&id, record.progress.clone());
    let hub = hub.clone();
    tokio::spawn(async move {
        run_task(&hub, meta, handle).await;
    });
    Ok(record)
}

/// 启动恢复：非终态任务从断点续跑（产物已落盘，跳过已完成阶段）
pub fn resume_pending(hub: &Arc<Hub>) {
    let Ok(entries) = std::fs::read_dir(tasks_root()) else {
        return;
    };
    for entry in entries.flatten() {
        let Some(id) = entry.file_name().to_str().map(String::from) else {
            continue;
        };
        let Some(rec) = read_record(&id) else {
            continue;
        };
        if !rec.progress.is_running() {
            continue;
        }
        // 与 create_task 同构：同步注册再 spawn，关闭竞态窗口
        let handle = hub.register(&id, rec.progress.clone());
        let h = hub.clone();
        tokio::spawn(async move {
            run_task(&h, rec.meta, handle).await;
        });
    }
}

/// 失败任务从断点重试（跳过已有产物的阶段）
pub fn retry_task(hub: &Arc<Hub>, id: &str) -> Result<TaskRecord, String> {
    if hub.is_running(id) {
        return Err("任务运行中".into());
    }
    let mut rec = read_record(id).ok_or("任务不存在")?;
    if rec.progress.stage == STAGE_DONE {
        return Err("任务已完成，无需重试".into());
    }
    if rec.progress.stage == STAGE_SELECTING {
        return Err("任务等待用户选择段落，无需重试".into());
    }
    if rec.progress.is_running() {
        return Err("任务状态不一致（磁盘运行态但不在运行表），请删除后重新制作".into());
    }
    rec.progress.error = None;
    rec.progress.finished_at = None;
    rec.progress.stage = STAGE_PENDING.into();
    write_record(&rec).map_err(|e| e.to_string())?;
    let handle = hub.register(id, rec.progress.clone());
    let meta = rec.meta.clone();
    let h = hub.clone();
    tokio::spawn(async move {
        run_task(&h, meta, handle).await;
    });
    Ok(rec)
}

pub fn delete_task(hub: &Hub, id: &str) -> Result<(), String> {
    if hub.is_running(id) {
        return Err("任务运行中，暂不能删除".into());
    }
    let Some(dir) = task_dir(id) else {
        return Err("非法的任务 id".into());
    };
    if !dir.is_dir() {
        return Err("任务不存在".into());
    }
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

/// 产物入人物库：card/ 目录复制到 db/人物/<slug>/，返回角色 id
pub fn save_task_to_library(id: &str, slug: String) -> Result<String, String> {
    let Some(dir) = task_dir(id) else {
        return Err("非法的任务 id".into());
    };
    let mut rec = read_record(id).ok_or("任务不存在")?;
    if rec.progress.stage != STAGE_DONE {
        return Err("任务未完成，尚无产物可保存".into());
    }
    let card_dir = dir.join("card");
    if !card_dir.is_dir() {
        return Err("任务没有产物（card/ 缺失）".into());
    };
    let characters = crate::assets::characters_dir();
    // 去重：目标目录或同名单文件已存在时追加 -2/-3 序号
    let mut final_id = slug;
    let mut n = 2;
    while characters.join(&final_id).exists()
        || characters.join(format!("{final_id}.md")).exists()
    {
        final_id = format!("{}-{n}", final_id);
        n += 1;
    }
    let dst = characters.join(&final_id);
    std::fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
    let Ok(entries) = std::fs::read_dir(&card_dir) else {
        return Err("读取产物失败".into());
    };
    let mut count = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else { continue };
        if !name_str.ends_with(".md") || name_str.ends_with(".tmp") {
            continue;
        }
        std::fs::copy(entry.path(), dst.join(&name)).map_err(|e| e.to_string())?;
        count += 1;
    }
    if count == 0 {
        let _ = std::fs::remove_dir(&dst);
        return Err("产物为空，未写入".into());
    }
    rec.progress.saved_character_id = Some(final_id.clone());
    let _ = write_record(&rec);
    Ok(final_id)
}

/// 读取产物文件（详情 API 共用）
pub fn read_card_files(id: &str) -> Vec<(String, String)> {
    let Some(dir) = task_dir(id) else {
        return Vec::new();
    };
    GENERATION_FILES
        .iter()
        .filter_map(|name| {
            let body = std::fs::read_to_string(dir.join("card").join(format!("{name}.md"))).ok()?;
            Some((name.to_string(), body))
        })
        .collect()
}

// ---- 后台执行（状态机主干） ----

async fn run_task(hub: &Arc<Hub>, meta: TaskMeta, handle: Arc<Mutex<Progress>>) {
    let id = meta.id.clone();
    // 从磁盘恢复进度（断点续跑 / 重试场景）；正常新建任务刚写入 pending
    let mut record = read_record(&id).unwrap_or(TaskRecord {
        progress: Progress {
            stage: STAGE_PENDING.into(),
            ..Default::default()
        },
        meta: meta.clone(),
    });

    let result = run_pipeline(&meta, &mut record, &handle).await;
    match result {
        Ok(()) if record.progress.stage == STAGE_SELECTING => {
            // 选择闸门：暂停不是完成——不写终态、不计 finished_at
        }
        Ok(()) => {
            record.progress.stage = STAGE_DONE.into();
            record.progress.finished_at = Some(now_secs());
        }
        Err(err) => {
            eprintln!("[deconstruct] 任务 {} 失败：{}", meta.id, err);
            record.progress.stage = STAGE_FAILED.into();
            record.progress.error = Some(err);
            record.progress.finished_at = Some(now_secs());
        }
    }
    *handle.lock().unwrap() = record.progress.clone();
    if let Err(e) = write_record(&record) {
        eprintln!("[deconstruct] 进度落盘失败 {e}");
    }
    hub.unregister(&id);
}

/// 进度同步：内存句柄 + 磁盘快照
fn sync(record: &TaskRecord, handle: &Arc<Mutex<Progress>>) {
    *handle.lock().unwrap() = record.progress.clone();
    if let Err(e) = write_record(record) {
        eprintln!("[deconstruct] 进度落盘失败 {e}");
    }
}

async fn run_pipeline(
    meta: &TaskMeta,
    record: &mut TaskRecord,
    handle: &Arc<Mutex<Progress>>,
) -> Result<(), String> {
    let dir = task_dir(&meta.id).expect("id 已校验");
    let set_stage = |stage: &str, record: &mut TaskRecord| {
        record.progress.stage = stage.to_string();
        sync(record, handle);
    };

    // 模型配置（fail fast：配置缺失是最常见的启动失败）
    let cfg = llm::load_llm_config().ok_or(
        "模型未配置：请在 ~/.config/shiro/config.toml 填写 [llm] 段（base_url / api_key / model）",
    )?;

    let content =
        std::fs::read_to_string(dir.join("source.txt")).map_err(|e| format!("读取原文失败：{e}"))?;

    // 1+2. 探测与切块（断点：chunks.json + probe.json 都在 → 读回跳过）
    let cached_chunks: Option<ChunkResult> = std::fs::read_to_string(dir.join("chunks.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let cached_probe: Option<ProbeResult> = std::fs::read_to_string(dir.join("probe.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let (chunks, probe_full) = match (cached_chunks, cached_probe) {
        (Some(c), Some(p)) => (c, p),
        _ => {
            set_stage(STAGE_PROBING, record);
            let (form, ratio) = probe::probe_form(&content);
            set_stage(STAGE_CHUNKING, record);
            let chunks = chunk::build_chunks(&content);
            let probe_full = ProbeResult {
                form: form.clone(),
                attribution_hit_ratio: ratio,
                citation_units: probe::citation_units(&chunks.segments),
            };
            let _ = atomic_write(
                &dir.join("chunks.json"),
                &serde_json::to_string_pretty(&chunks).map_err(|e| e.to_string())?,
            );
            let _ = atomic_write(
                &dir.join("probe.json"),
                &serde_json::to_string_pretty(&probe_full).map_err(|e| e.to_string())?,
            );
            (chunks, probe_full)
        }
    };
    let segments = chunks.segments.clone();
    let form = probe_full.form.clone();
    record.progress.form = Some(form.clone());
    record.progress.segment_count = segments.len();
    sync(record, handle);

    // 2.5 选择闸门：段清单写盘，等用户挑选（全选全收，不设上限）
    if record.progress.selected.is_none() {
        let list: Vec<SegmentInfo> = segments
            .iter()
            .enumerate()
            .map(|(i, seg)| SegmentInfo {
                index: i,
                label: seg.label.clone(),
                chars: seg.content.chars().count(),
                excerpt: seg_excerpt(&seg.content),
                has_name: evidence::name_matches(&meta.character, &meta.aliases, &seg.content),
            })
            .collect();
        let _ = atomic_write(
            &dir.join("segments.json"),
            &serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?,
        );
        set_stage(STAGE_SELECTING, record);
        return Ok(());
    }

    // 3. 逐段笔记（只跑用户选中的段；段失败降级，不阻断；断点：notes/<i>.json 已存在 → 读回跳过）
    set_stage(STAGE_NOTES, record);
    record.progress.notes_failed.clear();
    let selected: Vec<usize> = record.progress.selected.clone().unwrap_or_default();
    let mut notes: Vec<SegmentNote> = Vec::new();
    for i in selected.iter().copied() {
        let Some(seg) = segments.get(i) else { continue };
        let seg = seg.clone();
        if let Some(n) = std::fs::read_to_string(dir.join("notes").join(format!("{i}.json")))
            .ok()
            .and_then(|t| serde_json::from_str::<SegmentNote>(&t).ok())
        {
            notes.push(n);
            record.progress.notes_done = i + 1;
            sync(record, handle);
            continue;
        }
        let (sys, user) = prompts::notes_prompt(&seg.label, &seg.content);
        let mut note: Option<SegmentNote> = None;
        for _ in 0..=NOTES_MAX_RETRY {
            let text = llm::chat(&cfg, vec![msg("system", &sys), msg("user", &user)])
                .await
                .map_err(|e| {
                    let class = if e.retryable { "网络/上游" } else { "配置/协议" };
                    format!("段 {i}（{}）笔记调用失败（{class}）：{e}", seg.label)
                })?;
            let parsed = llm::extract_json(&text)
                .map_err(|e| e.to_string())
                .and_then(|v| serde_json::from_value::<SegmentNote>(v).map_err(|e| e.to_string()));
            match parsed {
                Ok(mut n) => {
                    prompts::backfill_segment(&mut n, &seg.label);
                    note = Some(n);
                    break;
                }
                Err(e) => eprintln!("[deconstruct] 段 {i} 笔记解析失败：{e}"),
            }
        }
        match note {
            Some(n) => {
                let _ = atomic_write(
                    &dir.join("notes").join(format!("{i}.json")),
                    &serde_json::to_string_pretty(&n).map_err(|e| e.to_string())?,
                );
                notes.push(n);
            }
            None => record.progress.notes_failed.push(i),
        }
        record.progress.notes_done = i + 1;
        sync(record, handle);
    }
    if notes.is_empty() {
        return Err("全部段落笔记解析失败，无法继续".into());
    }

    // 4. 证据包
    set_stage(STAGE_EVIDENCE, record);
    let pack: EvidencePack =
        evidence::build_pack(&meta.character, &meta.aliases, &segments, &notes, &form);
    let _ = atomic_write(
        &dir.join("evidence.json"),
        &serde_json::to_string_pretty(&pack).map_err(|e| e.to_string())?,
    );
    if pack.dialogues.is_empty() && pack.events.is_empty() && pack.mentions.is_empty() {
        return Err(format!(
            "未在剧本中找到「{}」的证据：检查角色名与别名是否与剧本中的写法一致",
            meta.character
        ));
    }

    // 5. 生成节点组（顺序固定：soul 锚定后续，index 最后压缩；断点：card/<name>.md 已存在 → 跳过）
    set_stage(STAGE_GENERATING, record);
    let card = dir.join("card");
    std::fs::create_dir_all(&card).map_err(|e| e.to_string())?;
    record.progress.files_done.clear();
    record.progress.current_file = None;
    sync(record, handle);

    let work = &meta.source_name;
    let character = &meta.character;
    let mut soul = String::new();

    for name in GENERATION_FILES {
        if let Ok(existing) = std::fs::read_to_string(card.join(format!("{name}.md"))) {
            if !record.progress.files_done.iter().any(|f| f == name) {
                record.progress.files_done.push(name.to_string());
                sync(record, handle);
            }
            if name == "soul" {
                soul = existing;
            }
            continue;
        }
        record.progress.current_file = Some(name.to_string());
        sync(record, handle);

        // 提示词重建（limit/index 的依赖从磁盘实时读取，断点下天然正确）
        let deps: Vec<(String, String)> = GENERATION_FILES[..5]
            .iter()
            .filter_map(|n| {
                std::fs::read_to_string(card.join(format!("{n}.md")))
                    .ok()
                    .map(|b| ((*n).to_string(), b))
            })
            .collect();
        let six: Vec<(String, String)> = GENERATION_FILES[..6]
            .iter()
            .filter_map(|n| {
                std::fs::read_to_string(card.join(format!("{n}.md")))
                    .ok()
                    .map(|b| ((*n).to_string(), b))
            })
            .collect();
        let dep_refs: Vec<(&str, &str)> =
            deps.iter().map(|(n, b)| (n.as_str(), b.as_str())).collect();
        let six_refs: Vec<(&str, &str)> =
            six.iter().map(|(n, b)| (n.as_str(), b.as_str())).collect();
        let (sys, user) = rebuild_prompt(
            name, work, character, &pack, &notes, &soul, &dep_refs, &six_refs,
        );
        if name == "index" {
            let body = llm::chat(&cfg, vec![msg("system", &sys), msg("user", &user)])
                .await
                .map_err(|e| format!("生成 index 失败：{e}"))?;
            let frontmatter = index_frontmatter(meta, &form);
            atomic_write(&card.join("index.md"), &format!("{frontmatter}\n{body}\n"))
                .map_err(|e| e.to_string())?;
        } else {
            chat_write(&cfg, &card, name, &sys, &user).await?;
        }
        record.progress.files_done.push(name.to_string());
        sync(record, handle);
        if name == "soul" {
            soul = std::fs::read_to_string(card.join("soul.md")).map_err(|e| e.to_string())?;
        }
    }
    record.progress.current_file = None;
    sync(record, handle);

    // 6. 引文回查（确定性 + 打回一次 + 删除降级；断点：report.json 已存在 → 读回跳过）
    if let Some(report) = std::fs::read_to_string(dir.join("report.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<VerifyReport>(&t).ok())
    {
        record.progress.verified = Some(report);
        sync(record, handle);
        return Ok(());
    }
    set_stage(STAGE_VERIFYING, record);
    let mut removed: Vec<RemovedQuote> = Vec::new();
    let mut passed = 0usize;
    for name in GENERATION_FILES {
        let path = card.join(format!("{name}.md"));
        let Ok(mut text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let mut failed = verify::verify_text(&text, &segments, &content);
        let mut regen = 0;
        while !failed.is_empty() && regen < QUOTE_REGEN_MAX {
            regen += 1;
            let failed_quotes: Vec<String> = failed.iter().map(|q| q.quote.clone()).collect();
            // 打回重建提示词：依赖从磁盘现读
            let deps: Vec<(String, String)> = GENERATION_FILES[..5]
                .iter()
                .filter_map(|n| {
                    std::fs::read_to_string(card.join(format!("{n}.md")))
                        .ok()
                        .map(|b| ((*n).to_string(), b))
                })
                .collect();
            let six: Vec<(String, String)> = GENERATION_FILES[..6]
                .iter()
                .filter_map(|n| {
                    std::fs::read_to_string(card.join(format!("{n}.md")))
                        .ok()
                        .map(|b| ((*n).to_string(), b))
                })
                .collect();
            let dep_refs: Vec<(&str, &str)> =
                deps.iter().map(|(n, b)| (n.as_str(), b.as_str())).collect();
            let six_refs: Vec<(&str, &str)> =
                six.iter().map(|(n, b)| (n.as_str(), b.as_str())).collect();
            let (sys, user) =
                rebuild_prompt(name, work, character, &pack, &notes, &soul, &dep_refs, &six_refs);
            let fixed_user = prompts::regen_with_failed_quotes(&user, &failed_quotes);
            match llm::chat(&cfg, vec![msg("system", &sys), msg("user", &fixed_user)]).await {
                Ok(new_text) => {
                    let new_failed = verify::verify_text(&new_text, &segments, &content);
                    if new_failed.len() < failed.len() {
                        text = new_text;
                        failed = new_failed;
                    }
                }
                Err(e) => eprintln!("[deconstruct] {name} 引文修复重生成失败：{e}"),
            }
        }
        let quotes = verify::extract_quotes(&text);
        if failed.is_empty() {
            passed += quotes.len();
        } else {
            passed += quotes.len().saturating_sub(failed.len());
            for q in &failed {
                removed.push(RemovedQuote {
                    file: name.to_string(),
                    quote: q.quote.clone(),
                    unit: q.unit.clone(),
                });
            }
            text = verify::strip_failed_quotes(&text, &failed);
        }
        let _ = atomic_write(&path, &text);
    }
    let report = VerifyReport { passed, removed };
    let _ = atomic_write(
        &dir.join("report.json"),
        &serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?,
    );
    record.progress.verified = Some(report);
    sync(record, handle);

    Ok(())
}

/// 单文件生成：chat → 原子写 card/<name>.md
async fn chat_write(
    cfg: &LlmConfig,
    card: &Path,
    name: &str,
    sys: &str,
    user: &str,
) -> Result<(), String> {
    let text = llm::chat(cfg, vec![msg("system", sys), msg("user", user)])
        .await
        .map_err(|e| format!("生成 {name} 失败：{e}"))?;
    atomic_write(&card.join(format!("{name}.md")), &text)
        .map_err(|e| format!("写入 {name} 失败：{e}"))
}

/// index.md 的 frontmatter（深卡元数据，程序生成不信任模型）
fn index_frontmatter(meta: &TaskMeta, form: &str) -> String {
    let aliases = if meta.aliases.is_empty() {
        String::new()
    } else {
        format!(
            "aliases: [{}]\n",
            meta.aliases
                .iter()
                .map(|a| format!("{a:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    format!(
        "---\nshiro_asset: character\ndepth: full\nname: {:?}\n{aliases}source_work:\n  title: {:?}\n  form: {form}\ntags: [拆书]\n---\n",
        meta.character, meta.source_name
    )
}

/// 引文修复打回时重建该文件的原始提示词
fn rebuild_prompt(
    name: &str,
    work: &str,
    character: &str,
    pack: &EvidencePack,
    notes: &[SegmentNote],
    soul: &str,
    dep_refs: &[(&str, &str)],
    six_refs: &[(&str, &str)],
) -> (String, String) {
    match name {
        "soul" => prompts::soul_prompt(work, character, pack, notes),
        "speech_patterns" => prompts::speech_prompt(work, character, pack, soul),
        "behavior_guide" => prompts::behavior_prompt(work, character, pack, soul),
        "relationship_dynamics" => prompts::relationship_prompt(work, character, pack, soul),
        "key_life_events" => prompts::events_prompt(work, character, pack, notes, soul),
        "limit" => prompts::limit_prompt(work, character, dep_refs),
        "index" => prompts::index_prompt(work, character, six_refs),
        other => (String::new(), format!("（未知文件 {other}，无法重建提示词）")),
    }
}

fn msg(role: &'static str, content: &str) -> ChatMessage {
    ChatMessage {
        role,
        content: content.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_format() {
        let meta = TaskMeta {
            id: "t1".into(),
            source_name: "链锯人".into(),
            character: "玛奇玛".into(),
            aliases: vec!["マキマ".into()],
            created_at: 1,
        };
        let fm = index_frontmatter(&meta, "script");
        assert!(fm.contains("shiro_asset: character"));
        assert!(fm.contains("depth: full"));
        assert!(fm.contains("name: \"玛奇玛\""));
        assert!(fm.contains("aliases: [\"マキマ\"]"));
        assert!(fm.contains("form: script"));
        // 无别名时不含 aliases 行
        let meta2 = TaskMeta {
            aliases: vec![],
            ..meta
        };
        assert!(!index_frontmatter(&meta2, "prose").contains("aliases"));
    }

    #[test]
    fn record_roundtrip() {
        let rec = TaskRecord {
            meta: TaskMeta {
                id: "x".into(),
                source_name: "s".into(),
                character: "c".into(),
                aliases: vec![],
                created_at: 42,
            },
            progress: Progress {
                stage: STAGE_DONE.into(),
                form: Some("script".into()),
                segment_count: 3,
                notes_done: 3,
                ..Default::default()
            },
        };
        let json = serde_json::to_string(&rec).unwrap();
        let back: TaskRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rec);
    }

    #[test]
    fn interrupted_marking() {
        let mut rec = TaskRecord {
            meta: TaskMeta {
                id: "x".into(),
                source_name: "s".into(),
                character: "c".into(),
                aliases: vec![],
                created_at: 1,
            },
            progress: Progress {
                stage: STAGE_NOTES.into(),
                ..Default::default()
            },
        };
        assert!(check_interrupted(&mut rec, false), "非运行中且不在内存 → 中断");
        assert_eq!(rec.progress.stage, STAGE_FAILED);
        // 在内存表（运行中）→ 不改写
        let mut rec2 = TaskRecord {
            progress: Progress {
                stage: STAGE_NOTES.into(),
                ..Default::default()
            },
            ..rec.clone()
        };
        rec2.meta.id = "y".into();
        assert!(!check_interrupted(&mut rec2, true));
        assert_eq!(rec2.progress.stage, STAGE_NOTES);
        // 终态不再标记
        assert!(!check_interrupted(&mut rec, false));
    }
}
