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
/// 用户中止（终态，可从断点重试）
pub const STAGE_ABORTED: &str = "aborted";
/// 应用重启导致的中断（暂停态：与选择闸门同属用户闸门，不自动续跑，等用户点继续）
pub const STAGE_INTERRUPTED: &str = "interrupted";

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
    /// 任务显示名（用户可改；空 = 前端用角色名兜底）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
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
    /// 正在生成笔记的段序号（并行时在飞多段；流程图呼吸态）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes_active: Vec<usize>,
    /// 已完成笔记的段序号（流程图逐格标绿用）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes_finished: Vec<usize>,
    /// 生成中的当前文件（并行波次可能多个；不带扩展名）
    #[serde(default)]
    pub current_files: Vec<String>,
    #[serde(default)]
    pub files_done: Vec<String>,
    /// 用户选中的段序号（选择闸门回填；空 = 尚未选择）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<Vec<usize>>,
    /// 中止/失败时所在的管线阶段（流程图定位红格用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_stage: Option<String>,
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
        matches!(self.stage.as_str(), STAGE_DONE | STAGE_FAILED | STAGE_ABORTED)
    }
    /// 运行态：非终态且不在用户闸门（selecting/interrupted 都是暂停点，不经用户点击不跑）
    pub fn is_running(&self) -> bool {
        !self.stage.is_empty()
            && !self.is_terminal()
            && self.stage != STAGE_SELECTING
            && self.stage != STAGE_INTERRUPTED
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

/// 中断检查：磁盘上运行态但不在内存表 → daemon 重启 → 标「已中断」暂停态（等用户继续，不自动烧 token）
fn check_interrupted(rec: &mut TaskRecord, running: bool) -> bool {
    if rec.progress.is_running() && !running {
        rec.progress.last_stage = Some(rec.progress.stage.clone());
        rec.progress.stage = STAGE_INTERRUPTED.into();
        rec.progress.error = Some("应用重启，任务已暂停——点「继续」从断点接着跑".into());
        if let Some(dir) = task_dir(&rec.meta.id) {
            abandon_pending_logs(&dir);
        }
        true
    } else {
        false
    }
}

/// 全局 LLM 并发闸门：上限每次获取时从 config.toml 现读（调节即生效，无需重启）。
/// std Mutex（持锁极短无 await）+ tokio Notify 唤醒
pub struct Limiter {
    cur: std::sync::Mutex<usize>,
    notify: tokio::sync::Notify,
}

impl Default for Limiter {
    fn default() -> Self {
        Self {
            cur: std::sync::Mutex::new(0),
            notify: tokio::sync::Notify::new(),
        }
    }
}

impl Limiter {
    pub async fn acquire(&self) {
        let max = crate::llm::max_concurrency() as usize;
        loop {
            {
                let mut g = self.cur.lock().unwrap();
                if *g < max {
                    *g += 1;
                    return;
                }
            }
            // Notified 在等待前创建 + notify 有存储许可，不漏唤醒
            self.notify.notified().await;
        }
    }
    pub fn release(&self) {
        {
            let mut g = self.cur.lock().unwrap();
            *g = g.saturating_sub(1);
        }
        self.notify.notify_one();
    }
}

// ---- Hub：内存中的运行任务表 ----

#[derive(Default)]
pub struct Hub {
    running: Mutex<HashMap<String, Arc<Mutex<Progress>>>>,
    /// 运行任务的 JoinHandle（中止用）
    handles: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    /// 全局 LLM 并发闸门（所有运行中任务共享）
    pub limiter: Arc<Limiter>,
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
        self.handles.lock().unwrap().remove(id);
    }

    /// 登记运行任务的 JoinHandle（spawn 后调用，中止用）
    fn register_handle(&self, id: &str, handle: tokio::task::JoinHandle<()>) {
        self.handles.lock().unwrap().insert(id.to_string(), handle);
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
        out.sort_by_key(|r| std::cmp::Reverse(r.meta.created_at));
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
    let jh = tokio::spawn(async move {
        run_task(&h, meta, handle).await;
    });
    hub.register_handle(id, jh);
    Ok(rec)
}

// ---- 创建与删除 ----

pub struct CreateParams {
    pub source_name: String,
    pub content: String,
    pub character: String,
    pub aliases: Vec<String>,
    /// 预选段（复制任务时携带原选择；None = 停在选择闸门）
    pub selected: Option<Vec<usize>>,
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
        title: None,
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
            selected: params.selected,
            ..Default::default()
        },
    };
    let dir = task_dir(&id).expect("id 已生成");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("source.txt"), &params.content).map_err(|e| e.to_string())?;
    write_record(&record).map_err(|e| e.to_string())?;

    // 同步注册后再 spawn：关闭「磁盘已写 pending、内存未注册」的竞态窗口
    let handle = hub.register(&id, record.progress.clone());
    let hub2 = hub.clone();
    let jh = tokio::spawn(async move {
        run_task(&hub2, meta, handle).await;
    });
    hub.register_handle(&id, jh);
    Ok(record)
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
    // interrupted（重启暂停）从断点继续，即「继续」按钮语义
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
    let jh = tokio::spawn(async move {
        run_task(&h, meta, handle).await;
    });
    hub.register_handle(id, jh);
    Ok(rec)
}

/// 中止运行中的任务：abort JoinHandle（LLM 流式请求随之断开），落盘 aborted 终态
pub fn abort_task(hub: &Arc<Hub>, id: &str) -> Result<TaskRecord, String> {
    if !hub.is_running(id) {
        return Err("任务不在运行中".into());
    }
    if let Some(jh) = hub.handles.lock().unwrap().remove(id) {
        jh.abort();
    }
    let mut rec = read_record(id).ok_or("任务不存在")?;
    rec.progress.last_stage = Some(rec.progress.stage.clone());
    rec.progress.stage = STAGE_ABORTED.into();
    rec.progress.error = Some("已被用户中止".into());
    rec.progress.finished_at = Some(now_secs());
    write_record(&rec).map_err(|e| e.to_string())?;
    hub.unregister(id);
    // 在飞调用的日志标记为已放弃（结果不会回写）
    if let Some(dir) = task_dir(id) {
        abandon_pending_logs(&dir);
    }
    Ok(rec)
}

/// 任务改名（显示名；空串 = 清除回退默认显示）
pub fn rename_task(id: &str, title: &str) -> Result<TaskRecord, String> {
    let mut rec = read_record(id).ok_or("任务不存在")?;
    let title = title.trim();
    rec.meta.title = if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    };
    write_record(&rec).map_err(|e| e.to_string())?;
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

    let result = run_pipeline(&meta, &mut record, &handle, hub).await;
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
            record.progress.last_stage = Some(record.progress.stage.clone());
            record.progress.stage = STAGE_FAILED.into();
            record.progress.error = Some(err);
            record.progress.finished_at = Some(now_secs());
            // 失败时在飞调用的日志标为已放弃
            if let Some(dir) = task_dir(&meta.id) {
                abandon_pending_logs(&dir);
            }
        }
    }
    *handle.lock().unwrap() = record.progress.clone();
    if let Err(e) = write_record(&record) {
        eprintln!("[deconstruct] 进度落盘失败 {e}");
    }
    // 中止竞态：abort 已写盘 aborted 时不覆盖
    if let Some(on_disk) = read_record(&id) {
        if on_disk.progress.stage == STAGE_ABORTED && record.progress.stage != STAGE_ABORTED {
            hub.unregister(&id);
            return;
        }
    }
    hub.unregister(&id);
}

// ---- LLM 调用日志：每次调用记全量（请求 + 响应/错误 + 耗时），供日志界面回放 ----

/// 下一个日志序号（按 logs/ 目录现存文件数递增）
fn next_log_seq(dir: &Path) -> u32 {
    std::fs::read_dir(dir.join("logs"))
        .map(|d| d.flatten().count() as u32 + 1)
        .unwrap_or(1)
}

/// 日志结构（段级一条，多次尝试追加在 attempts 数组里——重试不新开条目）
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct CallLog {
    seq: u32,
    at: u64,
    node: String,
    detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    segment: Option<usize>,
    request: serde_json::Value,
    /// 各次尝试（重试追加在尾部，错误历史全部保留）
    #[serde(default)]
    attempts: Vec<serde_json::Value>,
    /// 末次尝试状态（null = 进行中）
    ok: Option<bool>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    abandoned: bool,
    #[serde(default)]
    elapsed_ms: u64,
}

fn read_call_log(path: &Path) -> Option<CallLog> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
}

fn write_call_log(path: &Path, log: &CallLog) {
    let _ = atomic_write(path, &serde_json::to_string_pretty(log).unwrap_or_default());
}

/// 带日志与并发闸门的 LLM 调用：同 seq 的多次尝试追加到同一日志的 attempts 尾部。
/// seq 由调用方分配（并行场景下必须预分配，见 run_one_note 段级分配）
async fn call_llm_logged(
    cfg: &LlmConfig,
    sys: &str,
    user: &str,
    dir: &Path,
    node: &'static str,
    detail: &str,
    segment: Option<usize>,
    seq: u32,
) -> Result<llm::ChatOutput, llm::LlmError> {
    let started = std::time::Instant::now();
    let log_path = dir.join("logs").join(format!("{seq:03}-{node}.json"));

    // 创建或读取日志，追加 pending 尝试
    let mut log = read_call_log(&log_path).unwrap_or_else(|| CallLog {
        seq,
        at: now_secs(),
        node: node.to_string(),
        detail: detail.to_string(),
        segment,
        request: serde_json::json!({ "system": sys, "user": user }),
        attempts: vec![],
        ok: None,
        abandoned: false,
        elapsed_ms: 0,
    });
    let attempt_idx = log.attempts.len();
    log.attempts.push(serde_json::json!({
        "at": now_secs(),
        "ok": serde_json::Value::Null,
        "response": "",
        "thinking": "",
        "error": serde_json::Value::Null,
        "usage": serde_json::Value::Null,
        "fallback": false,
        "elapsed_ms": 0,
    }));
    write_call_log(&log_path, &log);

    // 增量节流回写（正文与思考同步追加；Arc<Mutex> 供两个回调共享）
    let partial = Arc::new(std::sync::Mutex::new(String::new()));
    let thinking_partial = Arc::new(std::sync::Mutex::new(String::new()));
    let last_flush = Arc::new(std::sync::Mutex::new(std::time::Instant::now()));

    let result = {
        let partial = partial.clone();
        let thinking_partial = thinking_partial.clone();
        let last_flush = last_flush.clone();
        llm::chat_stream(
            cfg,
            vec![msg("system", sys), msg("user", user)],
            |t| {
                partial.lock().unwrap().push_str(t);
                let mut lf = last_flush.lock().unwrap();
                if lf.elapsed() >= std::time::Duration::from_millis(250) {
                    *lf = std::time::Instant::now();
                    drop(lf);
                    log.attempts[attempt_idx]["response"] =
                        serde_json::Value::String(partial.lock().unwrap().clone());
                    log.attempts[attempt_idx]["thinking"] =
                        serde_json::Value::String(thinking_partial.lock().unwrap().clone());
                    log.attempts[attempt_idx]["elapsed_ms"] =
                        serde_json::Value::from(started.elapsed().as_millis() as u64);
                    write_call_log(&log_path, &log);
                }
            },
            |t| {
                thinking_partial.lock().unwrap().push_str(t);
            },
        )
        .await
    };

    match &result {
        Ok(out) => {
            log.attempts[attempt_idx]["ok"] = serde_json::Value::Bool(true);
            log.attempts[attempt_idx]["response"] =
                serde_json::Value::String(out.text.clone());
            log.attempts[attempt_idx]["thinking"] =
                serde_json::Value::String(out.thinking.clone());
            if let Some(u) = &out.usage {
                log.attempts[attempt_idx]["usage"] = serde_json::json!(u);
            }
            if out.fallback {
                log.attempts[attempt_idx]["fallback"] = serde_json::Value::Bool(true);
            }
            log.ok = Some(true);
        }
        Err(e) => {
            log.attempts[attempt_idx]["ok"] = serde_json::Value::Bool(false);
            log.attempts[attempt_idx]["error"] =
                serde_json::Value::String(e.message.clone());
            log.ok = Some(false);
        }
    }
    log.attempts[attempt_idx]["elapsed_ms"] =
        serde_json::Value::from(started.elapsed().as_millis() as u64);
    log.elapsed_ms = log
        .attempts
        .iter()
        .map(|a| a["elapsed_ms"].as_u64().unwrap_or(0))
        .sum();
    write_call_log(&log_path, &log);
    result
}

/// 单段笔记结果（并行任务出口）
enum NoteOutcome {
    Ok(SegmentNote),
    /// 重试耗尽后的降级失败（记 notes_failed，不阻断）
    Failed,
    /// 配置/协议类致命错误（整任务失败）
    Fatal(String),
}

/// 单段笔记执行：重试（可重试错误/解析失败）→ 解析 → 写盘
async fn run_one_note(
    cfg: &LlmConfig,
    dir: &Path,
    i: usize,
    label: &str,
    content: &str,
    seq_counter: Arc<std::sync::atomic::AtomicU32>,
) -> NoteOutcome {
    let (sys, user) = prompts::notes_prompt(label, content);
    // 段级一个日志序号：多次尝试追加到同一日志的 attempts 尾部（错误历史全保留）
    let seq = seq_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    for _ in 0..=NOTES_MAX_RETRY {
        let call = call_llm_logged(cfg, &sys, &user, dir, "notes", label, Some(i), seq).await;
        let text = match call {
            Ok(out) => out.text,
            Err(e) if e.retryable => {
                eprintln!("[deconstruct] 段 {i} 笔记调用重试：{e}");
                continue;
            }
            Err(e) => {
                return NoteOutcome::Fatal(format!(
                    "段 {i}（{label}）笔记调用失败（配置/协议）：{e}"
                ));
            }
        };
        match llm::extract_json(&text)
            .map_err(|e| e.to_string())
            .and_then(|v| serde_json::from_value::<SegmentNote>(v).map_err(|e| e.to_string()))
        {
            Ok(mut n) => {
                prompts::backfill_segment(&mut n, label);
                let _ = atomic_write(
                    &dir.join("notes").join(format!("{i}.json")),
                    &serde_json::to_string_pretty(&n).unwrap_or_default(),
                );
                return NoteOutcome::Ok(n);
            }
            Err(e) => eprintln!("[deconstruct] 段 {i} 笔记解析失败：{e}"),
        }
    }
    NoteOutcome::Failed
}

/// 日志摘要（列表用；请求/响应全文只给长度，正文走详情接口）
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct LogSummary {
    pub seq: u32,
    pub node: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment: Option<usize>,
    pub ok: Option<bool>,
    /// 已放弃：任务中止/中断/失败时在飞的调用，结果永远不会回写
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub abandoned: bool,
    /// 调用发起时间（epoch 秒；pending 条目前端据此秒表自增）
    pub at: u64,
    pub elapsed_ms: u64,
    pub request_chars: usize,
    pub response_chars: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 尝试次数（重试追加在同一日志内）
    pub attempt_count: usize,
    /// token 用量汇总（各次尝试求和；无 usage 数据时为 null）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<u64>,
    /// 缓存命中率（%）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_hit_rate: Option<u32>,
    /// 是否有思考过程内容
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_thinking: bool,
}

/// 读取任务的全部调用日志摘要（按序号升序）
pub fn read_logs(id: &str) -> Vec<LogSummary> {
    let Some(dir) = task_dir(id) else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(dir.join("logs")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let req = v.get("request").cloned().unwrap_or_default();
        let request_chars = req
            .get("system")
            .and_then(|x| x.as_str())
            .map(|s| s.chars().count())
            .unwrap_or(0)
            + req
                .get("user")
                .and_then(|x| x.as_str())
                .map(|s| s.chars().count())
                .unwrap_or(0);
        let attempts = v
            .get("attempts")
            .and_then(|a| a.as_array())
            .cloned()
            .unwrap_or_default();
        let sum_usage = |key: &str| -> u64 {
            attempts
                .iter()
                .map(|a| a.get("usage").and_then(|u| u.get(key)).and_then(|x| x.as_u64()).unwrap_or(0))
                .sum()
        };
        let input = sum_usage("input");
        let output = sum_usage("output");
        let cache_read = sum_usage("cache_read");
        let last_error = attempts
            .iter()
            .rev()
            .find_map(|a| a.get("error").and_then(|x| x.as_str()))
            .map(String::from);
        let has_thinking = attempts.iter().any(|a| {
            a.get("thinking")
                .and_then(|x| x.as_str())
                .map(|t| !t.is_empty())
                .unwrap_or(false)
        });
        let usage = crate::llm::Usage {
            input,
            output,
            cache_read,
            cache_write: sum_usage("cache_write"),
        };
        out.push(LogSummary {
            seq: v.get("seq").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
            node: v.get("node").and_then(|x| x.as_str()).unwrap_or("").into(),
            detail: v.get("detail").and_then(|x| x.as_str()).unwrap_or("").into(),
            segment: v.get("segment").and_then(|x| x.as_u64()).map(|x| x as usize),
            ok: v.get("ok").and_then(|x| x.as_bool()),
            abandoned: v.get("abandoned").and_then(|x| x.as_bool()).unwrap_or(false),
            at: v.get("at").and_then(|x| x.as_u64()).unwrap_or(0),
            elapsed_ms: v.get("elapsed_ms").and_then(|x| x.as_u64()).unwrap_or(0),
            request_chars,
            response_chars: attempts
                .iter()
                .map(|a| {
                    a.get("response")
                        .and_then(|x| x.as_str())
                        .map(|s| s.chars().count())
                        .unwrap_or(0)
                })
                .sum(),
            error: last_error,
            attempt_count: attempts.len().max(1),
            input_tokens: (input > 0).then_some(input),
            output_tokens: (output > 0).then_some(output),
            cache_read: (cache_read > 0).then_some(cache_read),
            cache_hit_rate: usage.cache_hit_rate(),
            has_thinking,
        });
    }
    out.sort_by_key(|l| l.seq);
    out
}

/// 把 pending 日志标为已放弃（任务中止/中断/失败时在飞的调用，结果永远不会回来）；
/// 幂等，反复调用安全。
pub fn abandon_pending_logs(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir.join("logs")) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let pending = v.get("ok").map(|x| x.is_null()).unwrap_or(false)
            && !v.get("abandoned").and_then(|x| x.as_bool()).unwrap_or(false);
        if pending {
            v["abandoned"] = serde_json::Value::Bool(true);
            if let Ok(json) = serde_json::to_string_pretty(&v) {
                let _ = atomic_write(&entry.path(), &json);
            }
        }
    }
}

/// 读取单条日志全文（按序号）
pub fn read_log(id: &str, seq: u32) -> Option<serde_json::Value> {
    let dir = task_dir(id)?;
    let entries = std::fs::read_dir(dir.join("logs")).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".json") {
            continue;
        }
        let text = std::fs::read_to_string(entry.path()).ok()?;
        let v: serde_json::Value = serde_json::from_str(&text).ok()?;
        if v.get("seq").and_then(|x| x.as_u64()) == Some(seq as u64) {
            return Some(v);
        }
    }
    None
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
    hub: &Arc<Hub>,
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

    // 3. 逐段笔记（只跑用户选中的段；滑动窗口并行——在飞数 ≤ 并发上限，
    //    排队段不算在飞；段失败降级不阻断；断点：notes/<i>.json 已存在 → 读回跳过）
    set_stage(STAGE_NOTES, record);
    record.progress.notes_failed.clear();
    record.progress.notes_active.clear();
    record.progress.notes_done = 0;
    let selected: Vec<usize> = record.progress.selected.clone().unwrap_or_default();
    let mut notes: Vec<(usize, SegmentNote)> = Vec::new();
    // 待生成队列（缓存命中的直接记完成，不进队列）
    let mut queue: Vec<(usize, String, String)> = Vec::new();
    for i in selected.iter().copied() {
        let Some(seg) = segments.get(i) else { continue };
        if let Some(n) = std::fs::read_to_string(dir.join("notes").join(format!("{i}.json")))
            .ok()
            .and_then(|t| serde_json::from_str::<SegmentNote>(&t).ok())
        {
            notes.push((i, n));
            record.progress.notes_finished.push(i);
            record.progress.notes_done += 1;
            continue;
        }
        queue.push((i, seg.label.clone(), seg.content.clone()));
    }
    sync(record, handle);

    // 日志序号计数器：每次调用（含每次重试）各占一号——失败尝试保留完整错误记录，不被覆盖
    let seq_counter = Arc::new(std::sync::atomic::AtomicU32::new(next_log_seq(&dir)));
    let mut set = tokio::task::JoinSet::new();
    let mut next = 0usize;
    loop {
        // 滑动窗口：补到并发上限才 spawn——spawn 即真在飞（UI 呼吸格据此着色）
        let max = llm::max_concurrency() as usize;
        let mut topped = false;
        while set.len() < max && next < queue.len() {
            let (i, label, content) = queue[next].clone();
            next += 1;
            let cfg2 = cfg.clone();
            let dir2 = dir.clone();
            let limiter = hub.limiter.clone();
            let counter2 = seq_counter.clone();
            record.progress.notes_active.push(i);
            set.spawn(async move {
                // 全局限流器：多任务并发时跨任务排队（单任务时窗口已保证 ≤ 上限）
                limiter.acquire().await;
                let out = run_one_note(&cfg2, &dir2, i, &label, &content, counter2).await;
                limiter.release();
                (i, out)
            });
            topped = true;
        }
        if topped {
            sync(record, handle);
        }
        if set.is_empty() {
            break;
        }
        match set.join_next().await {
            Some(Ok((i, NoteOutcome::Ok(note)))) => {
                notes.push((i, note));
                record.progress.notes_active.retain(|x| *x != i);
                record.progress.notes_finished.push(i);
            }
            Some(Ok((i, NoteOutcome::Failed))) => {
                record.progress.notes_active.retain(|x| *x != i);
                record.progress.notes_failed.push(i);
            }
            Some(Ok((_i, NoteOutcome::Fatal(msg)))) => {
                set.abort_all();
                return Err(msg);
            }
            Some(Err(e)) => {
                // JoinError：被中止或 panic——中止场景由 abort_task 收尾
                eprintln!("[deconstruct] 笔记任务 join 失败：{e}");
            }
            None => break,
        }
        record.progress.notes_done += 1;
        sync(record, handle);
    }
    notes.sort_by_key(|(i, _)| *i);
    let notes: Vec<SegmentNote> = notes.into_iter().map(|(_, n)| n).collect();
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
    record.progress.current_files.clear();
    sync(record, handle);

    let work = &meta.source_name;
    let character = &meta.character;
    let soul: String;

    // 生成顺序即一致性机制（波次并发）：
    //   soul 单独先行（锚定后续）→ speech/behavior/relationship/events 并行（只依赖 soul）
    //   → limit（依赖前五）→ index（压缩前六）
    // 断点：card/<name>.md 已存在 → 跳过
    let read_card = |name: &str| std::fs::read_to_string(card.join(format!("{name}.md"))).ok();
    let mark_done = |record: &mut TaskRecord, name: &str| {
        if !record.progress.files_done.iter().any(|f| f == name) {
            record.progress.files_done.push(name.to_string());
        }
        record.progress.current_files.retain(|f| f != name);
    };

    // 波次 1：soul
    match read_card("soul") {
        Some(c) => {
            soul = c;
            mark_done(record, "soul");
            sync(record, handle);
        }
        None => {
            record.progress.current_files = vec!["soul".into()];
            sync(record, handle);
            let seq = seq_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let (sys, user) = prompts::soul_prompt(work, character, &pack, &notes);
            let text = call_llm_logged(&cfg, &sys, &user, &dir, "generating", "soul", None, seq)
                .await
                .map_err(|e| format!("生成 soul 失败：{e}"))?.text;
            atomic_write(&card.join("soul.md"), &text).map_err(|e| e.to_string())?;
            soul = text;
            mark_done(record, "soul");
            sync(record, handle);
        }
    }

    // 波次 2：四文件并行（只依赖 soul）
    let wave2 = [
        "speech_patterns",
        "behavior_guide",
        "relationship_dynamics",
        "key_life_events",
    ];
    let missing2: Vec<&str> = wave2
        .iter()
        .copied()
        .filter(|n| read_card(n).is_none())
        .collect();
    for name in &wave2 {
        if read_card(name).is_some() {
            mark_done(record, name);
        }
    }
    if !missing2.is_empty() {
        record.progress.current_files = missing2.iter().map(|s| s.to_string()).collect();
        sync(record, handle);
        let mut set = tokio::task::JoinSet::new();
        for name in missing2.iter().copied() {
            let cfg2 = cfg.clone();
            let dir2 = dir.clone();
            let card2 = card.clone();
            let limiter = hub.limiter.clone();
            let soul2 = soul.clone();
            let pack2 = pack.clone();
            let notes2 = notes.clone();
            let work2 = work.to_string();
            let character2 = character.to_string();
            let counter2 = seq_counter.clone();
            set.spawn(async move {
                let seq = counter2.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                limiter.acquire().await;
                let (sys, user) = match name {
                    "speech_patterns" => prompts::speech_prompt(&work2, &character2, &pack2, &soul2),
                    "behavior_guide" => prompts::behavior_prompt(&work2, &character2, &pack2, &soul2),
                    "relationship_dynamics" => {
                        prompts::relationship_prompt(&work2, &character2, &pack2, &soul2)
                    }
                    _ => prompts::events_prompt(&work2, &character2, &pack2, &notes2, &soul2),
                };
                let out = call_llm_logged(
                    &cfg2, &sys, &user, &dir2, "generating", name, None, seq,
                )
                .await;
                limiter.release();
                match out {
                    Ok(out) => {
                        atomic_write(&card2.join(format!("{name}.md")), &out.text)
                            .map_err(|e| format!("写入 {name} 失败：{e}"))?;
                        Ok(name)
                    }
                    Err(e) => Err(format!("生成 {name} 失败：{e}")),
                }
            });
        }
        while let Some(res) = set.join_next().await {
            match res {
                Ok(Ok(name)) => {
                    mark_done(record, name);
                    sync(record, handle);
                }
                Ok(Err(msg)) => {
                    set.abort_all();
                    return Err(msg);
                }
                Err(e) => eprintln!("[deconstruct] 生成任务 join 失败：{e}"),
            }
        }
    }

    // 波次 3：limit（依赖前五文件的结论，从磁盘现读）
    if read_card("limit").is_some() {
        mark_done(record, "limit");
        sync(record, handle);
    } else {
        record.progress.current_files = vec!["limit".into()];
        sync(record, handle);
        let deps: Vec<(String, String)> = GENERATION_FILES[..5]
            .iter()
            .filter_map(|n| read_card(n).map(|b| (n.to_string(), b)))
            .collect();
        let dep_refs: Vec<(&str, &str)> =
            deps.iter().map(|(n, b)| (n.as_str(), b.as_str())).collect();
        let seq = seq_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (sys, user) = prompts::limit_prompt(work, character, &dep_refs);
        let text = call_llm_logged(&cfg, &sys, &user, &dir, "generating", "limit", None, seq)
            .await
            .map_err(|e| format!("生成 limit 失败：{e}"))?.text;
        atomic_write(&card.join("limit.md"), &text).map_err(|e| e.to_string())?;
        mark_done(record, "limit");
        sync(record, handle);
    }

    // 波次 4：index（对前六文件的压缩，frontmatter 程序拼）
    if read_card("index").is_some() {
        mark_done(record, "index");
        sync(record, handle);
    } else {
        record.progress.current_files = vec!["index".into()];
        sync(record, handle);
        let six: Vec<(String, String)> = GENERATION_FILES[..6]
            .iter()
            .filter_map(|n| read_card(n).map(|b| (n.to_string(), b)))
            .collect();
        let six_refs: Vec<(&str, &str)> =
            six.iter().map(|(n, b)| (n.as_str(), b.as_str())).collect();
        let seq = seq_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (sys, user) = prompts::index_prompt(work, character, &six_refs);
        let body = call_llm_logged(&cfg, &sys, &user, &dir, "generating", "index", None, seq)
            .await
            .map_err(|e| format!("生成 index 失败：{e}"))?.text;
        let frontmatter = index_frontmatter(meta, &form);
        atomic_write(&card.join("index.md"), &format!("{frontmatter}\n{body}\n"))
            .map_err(|e| e.to_string())?;
        mark_done(record, "index");
        sync(record, handle);
    }
    record.progress.current_files.clear();
    sync(record, handle);

    record.progress.current_files.clear();
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
            let (sys, user) = rebuild_prompt(
                name,
                &PromptCtx {
                    work,
                    character,
                    pack: &pack,
                    notes: &notes,
                    soul: &soul,
                    dep_refs: &dep_refs,
                    six_refs: &six_refs,
                },
            );
            let fixed_user = prompts::regen_with_failed_quotes(&user, &failed_quotes);
            let seq = seq_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let regen_call = call_llm_logged(
                &cfg, &sys, &fixed_user, &dir, "verifying", name, None, seq,
            )
            .await;
            match regen_call {
                Ok(new_text) => {
                    let new_failed = verify::verify_text(&new_text.text, &segments, &content);
                    if new_failed.len() < failed.len() {
                        text = new_text.text;
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

/// 生成提示词上下文（rebuild_prompt 打包参数）
struct PromptCtx<'a> {
    work: &'a str,
    character: &'a str,
    pack: &'a EvidencePack,
    notes: &'a [SegmentNote],
    soul: &'a str,
    dep_refs: &'a [(&'a str, &'a str)],
    six_refs: &'a [(&'a str, &'a str)],
}

/// 引文修复打回时重建该文件的原始提示词
fn rebuild_prompt(name: &str, ctx: &PromptCtx<'_>) -> (String, String) {
    match name {
        "soul" => prompts::soul_prompt(ctx.work, ctx.character, ctx.pack, ctx.notes),
        "speech_patterns" => prompts::speech_prompt(ctx.work, ctx.character, ctx.pack, ctx.soul),
        "behavior_guide" => prompts::behavior_prompt(ctx.work, ctx.character, ctx.pack, ctx.soul),
        "relationship_dynamics" => {
            prompts::relationship_prompt(ctx.work, ctx.character, ctx.pack, ctx.soul)
        }
        "key_life_events" => {
            prompts::events_prompt(ctx.work, ctx.character, ctx.pack, ctx.notes, ctx.soul)
        }
        "limit" => prompts::limit_prompt(ctx.work, ctx.character, ctx.dep_refs),
        "index" => prompts::index_prompt(ctx.work, ctx.character, ctx.six_refs),
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
            title: None,
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
                title: None,
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
                title: None,
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
        assert_eq!(rec.progress.stage, STAGE_INTERRUPTED);
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
