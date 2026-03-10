use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::PathBuf;

use crossterm::event::KeyCode;
use isi::objects::blob::hash_and_store_blob;
use isi::objects::commit::create_and_store_commit;
use isi::objects::tree::create_tree_object;
use isi::objects::types::TreeEntry;
use isi::store::ignore::IsiIgnore;
use isi::store::index::{add_to_index, read_index};
use isi::store::object_store::{read_object, save_to_objects};
use isi::store::refs::{read_head_commit, write_head_commit};
use isi::store::repo::find_root;

#[derive(PartialEq, Clone, Copy)]
pub enum Pane {
    Files,
    Log,
}

#[derive(PartialEq, Clone, Copy)]
pub enum FileSection {
    Unstaged,
    Untracked,
}

pub enum Mode {
    Normal,
    CommitInput,
}

#[derive(Clone, PartialEq)]
pub enum EntryKind {
    Modified,
    Deleted,
    Untracked,
}

#[derive(Clone)]
pub struct FileEntry {
    pub path: String,
    pub kind: EntryKind,
}

#[derive(Clone)]
pub struct CommitInfo {
    pub hash: String,
    pub message: String,
}

pub enum DiffLine {
    Header(String),
    Added(String),
    Removed(String),
    Context(String),
}

pub struct App {
    pub root: PathBuf,
    pub pane: Pane,
    pub file_section: FileSection,

    pub unstaged: Vec<FileEntry>,
    pub untracked: Vec<FileEntry>,
    pub log: Vec<CommitInfo>,

    pub unstaged_idx: usize,
    pub untracked_idx: usize,
    pub log_idx: usize,

    pub diff: Vec<DiffLine>,
    pub diff_scroll: usize,

    pub mode: Mode,
    pub commit_msg: String,
    pub status: Option<String>,
}

impl App {
    pub fn new() -> io::Result<Self> {
        let root = find_root()?;

        let mut app = App {
            root,
            pane: Pane::Files,
            file_section: FileSection::Unstaged,
            unstaged: vec![],
            untracked: vec![],
            log: vec![],
            unstaged_idx: 0,
            untracked_idx: 0,
            log_idx: 0,
            diff: vec![],
            diff_scroll: 0,
            mode: Mode::Normal,
            commit_msg: String::new(),
            status: None,
        };

        app.refresh()?;
        Ok(app)
    }

    pub fn refresh(&mut self) -> io::Result<()> {
        self.load_files()?;
        self.load_log()?;
        self.update_diff();
        Ok(())
    }

    fn load_files(&mut self) -> io::Result<()> {
        let entries = read_index()?;
        let ignore = IsiIgnore::load();
        let mut tracked: HashSet<String> = HashSet::new();
        let mut unstaged = Vec::new();

        for entry in &entries {
            tracked.insert(entry.path.clone());
            let file_path = self.root.join(&entry.path);

            if !file_path.exists() {
                unstaged.push(FileEntry { path: entry.path.clone(), kind: EntryKind::Deleted });
                continue;
            }

            let current = fs::read_to_string(&file_path).unwrap_or_default();
            let stored = read_object(&entry.hash).unwrap_or_default();
            if current != String::from_utf8_lossy(&stored).as_ref() {
                unstaged.push(FileEntry { path: entry.path.clone(), kind: EntryKind::Modified });
            }
        }

        let mut untracked = Vec::new();
        self.collect_untracked(&self.root.clone(), &ignore, &tracked, &mut untracked)?;

        if self.unstaged_idx >= unstaged.len().max(1) {
            self.unstaged_idx = unstaged.len().saturating_sub(1);
        }
        if self.untracked_idx >= untracked.len().max(1) {
            self.untracked_idx = untracked.len().saturating_sub(1);
        }

        self.unstaged = unstaged;
        self.untracked = untracked;
        Ok(())
    }

    fn collect_untracked(
        &self,
        dir: &std::path::Path,
        ignore: &IsiIgnore,
        tracked: &HashSet<String>,
        out: &mut Vec<FileEntry>,
    ) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let is_dir = path.is_dir();

            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name == ".isi" || name == ".git" {
                    continue;
                }
            }

            let abs = fs::canonicalize(&path)?;
            let rel = abs
                .strip_prefix(&self.root)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();

            if ignore.should_ignore(&rel, is_dir) {
                continue;
            }

            if is_dir {
                self.collect_untracked(&abs, ignore, tracked, out)?;
            } else if !tracked.contains(&rel) {
                out.push(FileEntry { path: rel, kind: EntryKind::Untracked });
            }
        }
        Ok(())
    }

    fn load_log(&mut self) -> io::Result<()> {
        let mut log = Vec::new();
        let mut hash = match read_head_commit()? {
            Some(h) => h,
            None => {
                self.log = log;
                return Ok(());
            }
        };

        for _ in 0..100 {
            let data = match read_object(&hash) {
                Ok(d) => d,
                Err(_) => break,
            };
            let text = String::from_utf8_lossy(&data).to_string();

            let message = text
                .lines()
                .skip_while(|l| !l.is_empty())
                .nth(1)
                .unwrap_or("")
                .to_string();

            let parent = text
                .lines()
                .find(|l| l.starts_with("parent "))
                .map(|l| l[7..].to_string());

            log.push(CommitInfo { hash: hash.clone(), message });

            match parent {
                Some(p) => hash = p,
                None => break,
            }
        }

        self.log = log;
        Ok(())
    }

    pub fn update_diff(&mut self) {
        self.diff_scroll = 0;
        self.diff = match self.pane {
            Pane::Files => self.compute_file_diff(),
            Pane::Log => self.compute_commit_diff(),
        };
    }

    fn compute_file_diff(&self) -> Vec<DiffLine> {
        let entry = match self.file_section {
            FileSection::Unstaged => self.unstaged.get(self.unstaged_idx),
            FileSection::Untracked => self.untracked.get(self.untracked_idx),
        };

        let entry = match entry {
            Some(e) => e,
            None => return vec![],
        };

        let file_path = self.root.join(&entry.path);

        match entry.kind {
            EntryKind::Deleted => {
                let entries = read_index().unwrap_or_default();
                let stored = entries
                    .iter()
                    .find(|e| e.path == entry.path)
                    .and_then(|e| read_object(&e.hash).ok())
                    .unwrap_or_default();
                let stored_str = String::from_utf8_lossy(&stored).to_string();

                let mut lines = vec![DiffLine::Header(format!("deleted: {}", entry.path))];
                for line in stored_str.lines() {
                    lines.push(DiffLine::Removed(line.to_string()));
                }
                lines
            }
            EntryKind::Untracked => {
                let content = fs::read_to_string(&file_path).unwrap_or_default();
                let mut lines = vec![DiffLine::Header(format!("untracked: {}", entry.path))];
                for line in content.lines() {
                    lines.push(DiffLine::Added(line.to_string()));
                }
                lines
            }
            EntryKind::Modified => {
                let entries = read_index().unwrap_or_default();
                let stored = entries
                    .iter()
                    .find(|e| e.path == entry.path)
                    .and_then(|e| read_object(&e.hash).ok())
                    .unwrap_or_default();
                let stored_str = String::from_utf8_lossy(&stored).to_string();
                let current = fs::read_to_string(&file_path).unwrap_or_default();

                let mut lines = vec![
                    DiffLine::Header(format!("--- {}", entry.path)),
                    DiffLine::Header("+++ working tree".to_string()),
                ];
                for d in diff::lines(&stored_str, &current) {
                    match d {
                        diff::Result::Left(l) => lines.push(DiffLine::Removed(l.to_string())),
                        diff::Result::Both(l, _) => lines.push(DiffLine::Context(l.to_string())),
                        diff::Result::Right(r) => lines.push(DiffLine::Added(r.to_string())),
                    }
                }
                lines
            }
        }
    }

    fn compute_commit_diff(&self) -> Vec<DiffLine> {
        let commit = match self.log.get(self.log_idx) {
            Some(c) => c,
            None => return vec![],
        };

        let data = match read_object(&commit.hash) {
            Ok(d) => d,
            Err(_) => return vec![],
        };

        let text = String::from_utf8_lossy(&data).to_string();
        let mut lines = vec![DiffLine::Header(format!("commit {}", commit.hash))];
        for line in text.lines() {
            lines.push(DiffLine::Context(line.to_string()));
        }
        lines
    }

    // Returns true if should quit
    pub fn handle_key(&mut self, key: KeyCode) -> io::Result<bool> {
        match self.mode {
            Mode::CommitInput => self.handle_commit_input(key),
            Mode::Normal => self.handle_normal(key),
        }
    }

    fn handle_normal(&mut self, key: KeyCode) -> io::Result<bool> {
        match key {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('c') => {
                self.mode = Mode::CommitInput;
                self.commit_msg.clear();
                self.status = None;
            }
            KeyCode::Char('a') => self.action_add()?,
            KeyCode::Tab => {
                self.pane = match self.pane {
                    Pane::Files => Pane::Log,
                    Pane::Log => Pane::Files,
                };
                self.update_diff();
            }
            KeyCode::Up | KeyCode::Char('k') => self.navigate(-1),
            KeyCode::Down | KeyCode::Char('j') => self.navigate(1),
            KeyCode::Char('J') => {
                self.diff_scroll = self.diff_scroll.saturating_add(3);
            }
            KeyCode::Char('K') => {
                self.diff_scroll = self.diff_scroll.saturating_sub(3);
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_commit_input(&mut self, key: KeyCode) -> io::Result<bool> {
        match key {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                if !self.commit_msg.is_empty() {
                    self.action_commit()?;
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.commit_msg.pop();
            }
            KeyCode::Char(c) => self.commit_msg.push(c),
            _ => {}
        }
        Ok(false)
    }

    fn navigate(&mut self, delta: i32) {
        match self.pane {
            Pane::Files => {
                let total_unstaged = self.unstaged.len();
                let total_untracked = self.untracked.len();

                match self.file_section {
                    FileSection::Unstaged => {
                        if delta < 0 {
                            if self.unstaged_idx > 0 {
                                self.unstaged_idx -= 1;
                            }
                        } else {
                            let next = self.unstaged_idx + 1;
                            if next < total_unstaged {
                                self.unstaged_idx = next;
                            } else if total_untracked > 0 {
                                self.file_section = FileSection::Untracked;
                                self.untracked_idx = 0;
                            }
                        }
                    }
                    FileSection::Untracked => {
                        if delta < 0 {
                            if self.untracked_idx > 0 {
                                self.untracked_idx -= 1;
                            } else if total_unstaged > 0 {
                                self.file_section = FileSection::Unstaged;
                                self.unstaged_idx = total_unstaged.saturating_sub(1);
                            }
                        } else {
                            let next = self.untracked_idx + 1;
                            if next < total_untracked {
                                self.untracked_idx = next;
                            }
                        }
                    }
                }
            }
            Pane::Log => {
                if delta < 0 {
                    self.log_idx = self.log_idx.saturating_sub(1);
                } else {
                    let next = self.log_idx + 1;
                    if next < self.log.len() {
                        self.log_idx = next;
                    }
                }
            }
        }
        self.update_diff();
    }

    fn action_add(&mut self) -> io::Result<()> {
        let entry = match self.file_section {
            FileSection::Unstaged => self.unstaged.get(self.unstaged_idx).cloned(),
            FileSection::Untracked => self.untracked.get(self.untracked_idx).cloned(),
        };

        if let Some(entry) = entry {
            if entry.kind != EntryKind::Deleted {
                let abs_path = self.root.join(&entry.path);
                let hash = hash_and_store_blob(abs_path.to_str().unwrap())?;
                add_to_index(&hash, &entry.path)?;
                self.status = Some(format!("added: {}", entry.path));
            }
            self.refresh()?;
        }
        Ok(())
    }

    fn action_commit(&mut self) -> io::Result<()> {
        let entries = read_index()?;

        if entries.is_empty() {
            self.status = Some("nothing to commit".to_string());
            return Ok(());
        }

        let tree_entries: Vec<TreeEntry> = entries
            .iter()
            .map(|e| TreeEntry {
                mode: "100644".to_string(),
                name: e.path.clone(),
                hash_hex: e.hash.clone(),
            })
            .collect();

        let (tree_hash, tree_data) = create_tree_object(tree_entries)?;
        save_to_objects(&tree_hash, &tree_data)?;

        let parent = read_head_commit()?;
        let commit_hash =
            create_and_store_commit(&tree_hash, parent.as_deref(), &self.commit_msg)?;
        write_head_commit(&commit_hash)?;

        self.status = Some(format!("[{}] {}", &commit_hash[..7], self.commit_msg));
        self.commit_msg.clear();
        self.refresh()?;
        Ok(())
    }
}
