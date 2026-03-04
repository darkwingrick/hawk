# Plan: Replace Empty Workspace When Opening Folder

## Problem
When multi-workspace is enabled:
1. Click '+' to create a new empty workspace → creates "Workspace 4" (expected)
2. Open a folder → creates another new workspace instead of replacing "Workspace 4"

## Root Cause
In [`new_local`](crates/workspace/src/workspace.rs:1807), when `requesting_window` is provided, the code ALWAYS adds a new workspace via [`multi_workspace.activate()`](crates/workspace/src/multi_workspace.rs:303), regardless of whether the current workspace is empty.

## Solution
When opening a folder and the current workspace is "empty" (no worktrees, no dirty items), replace the current workspace instead of adding a new one.

## Implementation Steps

### Step 1: Add a method to replace workspace in MultiWorkspace
Location: [`crates/workspace/src/multi_workspace.rs`](crates/workspace/src/multi_workspace.rs)

Add a new method `replace_workspace` that:
- Takes a new workspace entity
- Replaces the workspace at `active_workspace_index` instead of adding
- Only works when multi_workspace is enabled

### Step 2: Modify new_local to use replacement when appropriate
Location: [`crates/workspace/src/workspace.rs`](crates/workspace/src/workspace.rs:1807)

In the `new_local` function:
- Check if the current workspace is "empty" (no worktrees, no dirty items)
- If empty and we have a requesting_window, use the new `replace_workspace` method instead of `activate`

### Step 3: Pass empty state information to new_local
The `open_workspace_for_paths` function already checks for `has_worktree` and `has_dirty_items`. This information needs to be passed to `new_local` so it can make the right decision.

## Key Code Locations

| File | Line | Description |
|------|------|-------------|
| workspace.rs | 2950-2986 | `open_workspace_for_paths` - decides whether to replace |
| workspace.rs | 1807-1836 | `new_local` - creates workspace, currently always adds |
| multi_workspace.rs | 303-316 | `activate` - always adds when multi_workspace enabled |
| multi_workspace.rs | 541-560 | `create_workspace` - for '+' button |

## Testing Considerations
- Verify '+' button still creates new workspace (adds)
- Verify opening folder in empty workspace replaces it
- Verify opening folder in workspace with worktree creates new workspace
- Verify unsaved files prevent replacement
