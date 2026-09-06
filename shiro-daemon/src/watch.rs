//! 项目目录文件监听：notify 递归监听，防抖 300ms 收集变更路径后按项目广播。
//! 消费方：api.rs 的 SSE 接口（编辑器外部变动重载），后续 AI 自动写稿可直接订阅同一 Hub。
//! 生命周期与 SSE 订阅绑定（引用计数）：首个订阅者建立监听，最后一个断开即停止。

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};

/// 单次推送的变更集：项目内相对路径（'/' 分隔）；空数组 = 事件滞后溢出，订阅方应全量刷新
pub type ChangeSet = Vec<String>;

struct WatchEntry {
    tx: broadcast::Sender<ChangeSet>,
    /// 监听句柄，随 Entry 移除而 drop（即停止监听）；建监听失败时为 None（订阅仍可用，仅无事件）
    _watcher: Option<RecommendedWatcher>,
    subscribers: usize,
}

#[derive(Clone, Default)]
pub struct WatchHub {
    inner: Arc<Mutex<HashMap<PathBuf, WatchEntry>>>,
}

/// 订阅守卫：随 SSE 流存活，drop 时释放订阅（归零则停监听）
pub struct SubscriptionGuard {
    hub: WatchHub,
    root: PathBuf,
}

impl Drop for SubscriptionGuard {
    fn drop(&mut self) {
        let mut map = self.hub.inner.lock().unwrap();
        if let Some(entry) = map.get_mut(&self.root) {
            entry.subscribers -= 1;
            if entry.subscribers == 0 {
                map.remove(&self.root);
            }
        }
    }
}

impl WatchHub {
    /// 订阅项目目录变动；返回广播接收器与订阅守卫（守卫 drop 即退订）
    pub fn subscribe(&self, root: &Path) -> (broadcast::Receiver<ChangeSet>, SubscriptionGuard) {
        let mut map = self.inner.lock().unwrap();
        let entry = map
            .entry(root.to_path_buf())
            .or_insert_with(|| spawn_watch(root));
        entry.subscribers += 1;
        (
            entry.tx.subscribe(),
            SubscriptionGuard {
                hub: self.clone(),
                root: root.to_path_buf(),
            },
        )
    }
}

/// 绝对路径 → 项目内相对路径（'/' 分隔）。
/// 过滤：`.` 开头的路径段（.shiro/.git 等内部目录）、原子写临时文件、项目外路径。
fn rel_path(root: &Path, p: &Path) -> Option<String> {
    let rel = p.strip_prefix(root).ok()?;
    let mut out = String::new();
    for seg in rel.iter() {
        let s = seg.to_string_lossy();
        if s.starts_with('.') {
            return None;
        }
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(&s);
    }
    if out.ends_with(".shiro-tmp") {
        return None;
    }
    Some(out)
}

const DEBOUNCE: Duration = Duration::from_millis(300);

fn spawn_watch(root: &Path) -> WatchEntry {
    let (tx, _) = broadcast::channel(64);
    let (raw_tx, mut raw_rx) = mpsc::unbounded_channel::<PathBuf>();
    let watcher = match RecommendedWatcher::new(
        move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                for p in event.paths {
                    let _ = raw_tx.send(p);
                }
            }
        },
        Config::default(),
    ) {
        Ok(mut w) => match w.watch(root, RecursiveMode::Recursive) {
            Ok(()) => Some(w),
            Err(e) => {
                eprintln!("[shiro-daemon] 监听 {:?} 失败: {e}", root);
                None
            }
        },
        Err(e) => {
            eprintln!("[shiro-daemon] 创建文件监听器失败: {e}");
            None
        }
    };

    // 防抖任务：静默期内持续收集，期满后过滤广播；raw_tx 随 watcher drop 关闭时退出
    let task_root = root.to_path_buf();
    let task_tx = tx.clone();
    tokio::spawn(async move {
        let mut pending: HashSet<PathBuf> = HashSet::new();
        loop {
            let Some(p) = raw_rx.recv().await else { break };
            pending.insert(p);
            loop {
                match tokio::time::timeout(DEBOUNCE, raw_rx.recv()).await {
                    Ok(Some(p)) => {
                        pending.insert(p);
                    }
                    Ok(None) => return,
                    Err(_) => break,
                }
            }
            let changed: ChangeSet = pending
                .drain()
                .filter_map(|p| rel_path(&task_root, &p))
                .collect();
            if !changed.is_empty() {
                let _ = task_tx.send(changed);
            }
        }
    });

    WatchEntry {
        tx,
        _watcher: watcher,
        subscribers: 0,
    }
}
