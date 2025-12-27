use anyhow::Result;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

pub enum WatcherEvent {
    Changed,
}

pub struct FileWatcher {
    _watcher: RecommendedWatcher,
    pub receiver: Receiver<WatcherEvent>,
}

impl FileWatcher {
    pub fn new(repo_path: &Path) -> Result<Self> {
        let (tx, rx) = channel();

        let event_tx = tx.clone();
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if res.is_ok() {
                    let _ = event_tx.send(WatcherEvent::Changed);
                }
            },
            Config::default().with_poll_interval(Duration::from_secs(2)),
        )?;

        watcher.watch(repo_path, RecursiveMode::Recursive)?;

        let git_dir = repo_path.join(".git");
        if git_dir.exists() {
            let index_path = git_dir.join("index");
            if index_path.exists() {
                let _ = watcher.watch(&index_path, RecursiveMode::NonRecursive);
            }
            let head_path = git_dir.join("HEAD");
            if head_path.exists() {
                let _ = watcher.watch(&head_path, RecursiveMode::NonRecursive);
            }
        }

        Ok(Self {
            _watcher: watcher,
            receiver: rx,
        })
    }
}
