# Sidebar Workspace Enhancements Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enhance the MultiWorkspace sidebar with terminal activity animation and a more distinct active workspace highlight.

**Architecture:** Feature 2 is a one-line styling change in `ThreadItem`. Feature 1 requires adding a `has_busy_terminal` method to `WorkspaceThreadEntry` that queries the `TerminalPanel` for any terminal with a running task, then ORing that with the existing agent-running state. The sidebar crate needs a new dependency on `terminal_view` and `terminal`.

**Tech Stack:** Rust, GPUI framework, existing `ThreadItem`/`SpinnerLabel` UI components.

---

## Task 1: Stronger Active Workspace Highlight

**Files:**
- Modify: `crates/ui/src/components/ai/thread_item.rs:249`

**Step 1: Change the selected styling in ThreadItem::render()**

In `crates/ui/src/components/ai/thread_item.rs`, find line 249:

```rust
.when(self.selected, |s| s.bg(clr.element_active))
```

Replace with:

```rust
.when(self.selected, |s| {
    s.bg(clr.element_selected)
        .border_l_2()
        .border_color(clr.border_focused)
})
```

This uses `element_selected` (stronger background) and adds a 2px blue left border bar.

**Step 2: Verify it compiles**

Run: `cargo check -p ui`
Expected: compiles with no errors

**Step 3: Visual verification**

Run: `cargo run`
Expected: The active workspace in the sidebar now has a visible blue left border and a stronger background highlight.

**Step 4: Commit**

```bash
git add crates/ui/src/components/ai/thread_item.rs
git commit -m "Strengthen active workspace highlight in sidebar ThreadItem"
```

---

## Task 2: Add `has_busy_terminal` helper to sidebar

**Files:**
- Modify: `crates/sidebar/Cargo.toml` (add `terminal_view` and `terminal` dependencies)
- Modify: `crates/sidebar/src/sidebar.rs` (add import + helper method on `WorkspaceThreadEntry`)

**Step 1: Add dependencies to sidebar Cargo.toml**

In `crates/sidebar/Cargo.toml`, add to `[dependencies]`:

```toml
terminal.workspace = true
terminal_view.workspace = true
```

**Step 2: Add the `has_busy_terminal` method**

In `crates/sidebar/src/sidebar.rs`, add these imports at the top:

```rust
use terminal::TaskStatus;
use terminal_view::TerminalPanel;
```

Then add this method to `impl WorkspaceThreadEntry` (after the `thread_info` method around line 119):

```rust
fn has_busy_terminal(workspace: &Entity<Workspace>, cx: &App) -> bool {
    let Some(terminal_panel) = workspace.read(cx).panel::<TerminalPanel>(cx) else {
        return false;
    };
    let terminal_panel_ref = terminal_panel.read(cx);
    terminal_panel_ref
        .center
        .panes()
        .iter()
        .any(|pane| {
            pane.read(cx).items().any(|item| {
                item.downcast::<terminal_view::TerminalView>()
                    .map_or(false, |tv| {
                        tv.read(cx)
                            .terminal()
                            .read(cx)
                            .task()
                            .map_or(false, |task| task.status == TaskStatus::Running)
                    })
            })
        })
}
```

**Step 3: Verify it compiles**

Run: `cargo check -p sidebar`
Expected: compiles (the method is defined but not yet called)

**Step 4: Commit**

```bash
git add crates/sidebar/Cargo.toml crates/sidebar/src/sidebar.rs
git commit -m "Add has_busy_terminal helper to sidebar WorkspaceThreadEntry"
```

---

## Task 3: Wire terminal busy state into WorkspaceThreadEntry and ThreadItem rendering

**Files:**
- Modify: `crates/sidebar/src/sidebar.rs` (add field to `WorkspaceThreadEntry`, set it in `new()`, use it in `render_match()`)

**Step 1: Add `has_busy_terminal` field to `WorkspaceThreadEntry`**

In `crates/sidebar/src/sidebar.rs`, add a new field to the `WorkspaceThreadEntry` struct (around line 49):

```rust
struct WorkspaceThreadEntry {
    index: usize,
    worktree_label: SharedString,
    git_branch: Option<SharedString>,
    full_path: SharedString,
    thread_info: Option<AgentThreadInfo>,
    has_busy_terminal: bool,
}
```

**Step 2: Set the field in `WorkspaceThreadEntry::new()`**

In the `new()` method, compute the value and include it in the struct construction (around line 83-92):

```rust
let git_branch = Self::active_git_branch(workspace, cx);
let thread_info = Self::thread_info(workspace, cx);
let has_busy_terminal = Self::has_busy_terminal(workspace, cx);

Self {
    index,
    worktree_label,
    git_branch,
    full_path,
    thread_info,
    has_busy_terminal,
}
```

**Step 3: Use `has_busy_terminal` in `render_match()`**

In `render_match()`, find the block around lines 611-614 where `running` is computed:

```rust
let running = matches!(
    status,
    AgentThreadStatus::Running | AgentThreadStatus::WaitingForConfirmation
);
```

Change it to OR in the terminal state:

```rust
let agent_running = matches!(
    status,
    AgentThreadStatus::Running | AgentThreadStatus::WaitingForConfirmation
);
let running = agent_running || thread_entry.has_busy_terminal;
```

**Step 4: Verify it compiles**

Run: `cargo check -p sidebar`
Expected: compiles with no errors

**Step 5: Visual verification**

Run: `cargo run`
Expected: When you run a task in a terminal (e.g. `sleep 10`), the sidebar workspace entry shows the spinner animation. When the command finishes, the spinner stops.

Note: The spinner will only show for *tasks* run via Zed's task system (which use `TaskState`), not arbitrary shell commands typed into a plain terminal. This is because `Terminal.task()` only has state for Zed-spawned tasks. If you want to detect any foreground process (including manual `cargo build` etc.), that would require checking the PTY's foreground process group, which is a larger change.

**Step 6: Commit**

```bash
git add crates/sidebar/src/sidebar.rs
git commit -m "Show spinner in sidebar when workspace terminal has running task"
```

---

## Task 4: Check for accessibility of `center` field on TerminalPanel

**Files:**
- Potentially modify: `crates/terminal_view/src/terminal_panel.rs` (if `center` is not `pub`)

**Step 1: Verify `center` field visibility**

The `center` field on `TerminalPanel` is `pub(crate)`. Since the sidebar crate is external, we need to add a public method. Add to `impl TerminalPanel`:

```rust
pub fn panes(&self) -> &[Entity<Pane>] {
    self.center.panes()
}
```

Then update the `has_busy_terminal` helper in sidebar to use `terminal_panel_ref.panes()` instead of `terminal_panel_ref.center.panes()`.

**Step 2: Verify it compiles**

Run: `cargo check -p sidebar`
Expected: compiles with no errors

**Step 3: Commit**

```bash
git add crates/terminal_view/src/terminal_panel.rs crates/sidebar/src/sidebar.rs
git commit -m "Expose TerminalPanel::panes() for cross-crate access"
```
