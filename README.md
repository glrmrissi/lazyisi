# lazyisi

A terminal UI for [isi](https://github.com/glrmrissi/isi) — a minimalist Git-like version control system written in Rust. Inspired by [lazygit](https://github.com/jesseduffield/lazygit).

## Requirements

`lazyisi` is a frontend for `isi`. You need to have an `isi` repository initialized in your project before using it.

## Installation

```bash
# 1. Install isi first (required)
git clone https://github.com/glrmrissi/isi
cd isi
cargo install --path .

# 2. Install lazyisi
git clone https://github.com/glrmrissi/lazyisi
cd lazyisi
cargo install --path .
```

Both binaries are placed in `~/.cargo/bin/` and work from any directory.

## Usage

Navigate to any directory inside an `isi` repository and run:

```bash
lazyisi
```

## Interface

```
┌─ Files ──────────┬─ Diff ───────────────────────────────────────┐
│ Unstaged (1):    │ --- src/main.rs                              │
│ > M src/main.rs  │ +++ working tree                             │
│ Untracked (1):   │   fn main() {                                │
│   notes.txt      │ -     println!("hello");                     │
├─ Log ────────────│ +     println!("world");                     │
│ > a1b2c3d msg    │   }                                          │
│   e4f5a6b msg    │                                              │
├─ Tree @ a1b2c3d ─│                                              │
│   blob main.rs   │                                              │
│ > blob lib.rs    │                                              │
└──────────────────┴──────────────────────────────────────────────┘
  q:quit  a:add  c:commit  Tab:switch pane  ↑↓/jk:navigate  JK:scroll diff
```

The interface is split into two columns:

**Left column — three panels stacked vertically:**
- **Files** — shows modified tracked files (`M`) and untracked files (`?`). Deleted files are shown in red (`D`).
- **Log** — lists commits from newest to oldest, each showing its short hash and message.
- **Tree** — shows the file snapshot of the commit selected in the Log panel.

**Right column:**
- **Diff** — changes based on the focused panel:
  - **Files focused**: line-level diff between the stored blob and the current working tree file.
  - **Log focused**: commit metadata (tree hash, parent, author, message).
  - **Tree focused**: the full content of the selected file as it was at that commit.

## Keybindings

| Key | Action |
|-----|--------|
| `q` | Quit |
| `Tab` | Cycle focus: Files → Log → Tree → Files |
| `j` / `↓` | Move selection down |
| `k` / `↑` | Move selection up |
| `a` | Add selected file to the index (stage it) |
| `c` | Open commit message input |
| `Enter` | Confirm commit (inside commit input) |
| `Esc` | Cancel commit input |
| `J` | Scroll diff panel down |
| `K` | Scroll diff panel up |

## Workflow

```bash
# 1. Initialize a repository
isi init

# 2. Open the TUI
lazyisi

# 3. Navigate to a file in the Files panel
# 4. Press `a` to stage it
# 5. Press `c`, type a message, press Enter to commit
# 6. Press Tab twice to reach the Tree panel
# 7. Navigate the files of any past commit and see their content in the Diff panel
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| `isi` | Core version control logic (objects, index, refs) |
| `ratatui` | Terminal UI framework |
| `crossterm` | Cross-platform terminal input/output |
| `diff` | Line-level diff algorithm |
