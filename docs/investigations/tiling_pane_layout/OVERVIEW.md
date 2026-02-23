---
status: ONGOING
trigger: "Exploring how to introduce tabbed editing with tiling window manager-style pane splitting — moving tabs between panes using directional commands, with a tree-based layout model"
proposed_chunks:
  - prompt: "Binary pane tree data model: Implement a binary pane layout tree — Leaf(Pane) | Split { direction, ratio, first, second }. Support both horizontal and vertical splits. Implement layout calculation (rect splitting with ratios). Pane owns its own tabs. Add tree traversal helpers: path-to-pane, find-target-in-direction, nearest-leaf. Comprehensive unit tests for layout and traversal."
    chunk_directory: tiling_tree_model
    depends_on: []
  - prompt: "Directional tab movement operations: Implement move_tab(pane_id, direction) on the tree — find-target via tree walk, execute move (remove from source, add to target or split). Implement empty-pane cleanup (promote sibling when pane becomes empty). Unit tests for all move scenarios: split creation, move to existing neighbor, tree collapse on empty."
    chunk_directory: tiling_tab_movement
    depends_on: [0]
  - prompt: "Workspace pane integration: Replace Workspace.tabs / Workspace.active_tab with Workspace.pane_root / Workspace.active_pane_id. Update EditorState delegate methods to resolve through the active pane. Update tab creation, closing, and cycling to operate on the active pane. Update terminal polling to traverse all panes. Maintain backward compatibility for single-pane workspaces. Refactor handle_mouse to flip y once at entry and transform to pane-local coordinates at the dispatch point, eliminating the ad-hoc coordinate transform pipeline that has caused 7+ bug-fix chunks historically."
    chunk_directory: tiling_workspace_integration
    depends_on: [1]
  - prompt: "Multi-pane rendering: Update the renderer to iterate pane rectangles and render each pane independently — tab bar, content area, cursor, selection. Add clip rectangles per pane. Render divider lines between panes. Handle the focused-pane visual indicator. Each pane content renderer receives pane-local geometry so pixel_to_buffer_position becomes a pure function that never sees window-global offsets."
    chunk_directory: tiling_multi_pane_render
    depends_on: [2]
  - prompt: "Pane focus navigation and keybindings: Wire Cmd+Shift+Arrow to move_tab(active_pane, direction). Wire Cmd+Option+Arrow to focus-switch between panes. Mouse clicks within a pane rectangle set that pane as focused. Hit-testing uses PaneRect values computed by the layout algorithm in screen space — no per-handler offset math."
    chunk_directory: tiling_focus_keybindings
    depends_on: [3]
created_after: ["scroll_perf_deep", "syntax_highlight_issues"]
---

## Trigger

lite-edit currently has a flat tab model within each workspace (`workspace.rs`) — a single `Vec<Tab>` with one active tab index. There is no pane splitting or tiling support.

The desired experience draws from tiling window managers (i3, Sway, Hyprland, bspwm):

1. **Starting state**: A single pane fills the content area, containing one or more tabs.
2. **Directional tab movement**: With multiple tabs open, pressing `Cmd+Shift+→` moves the focused tab to a pane on the right — either an existing pane in that direction, or a newly created pane via splitting the current tree node.
3. **Recursive splitting**: `Cmd+Shift+↓` in the new right pane moves the focused tab into a pane below it, splitting vertically within that branch of the tree. The layout tree grows organically from user actions.
4. **Tree semantics**: Each internal node is either a horizontal split or a vertical split. Leaf nodes are panes containing tabs. Moving a tab in a direction either targets an unambiguous neighbor or splits the current node's parent to create one.

This investigation explores the data model, tree operations, layout algorithm, focus navigation, and keybinding design needed to make this work.

## Success Criteria

1. **Define the pane tree data model** with enough precision to implement: node types, split directions, how tabs are stored, how pane focus is tracked.
2. **Define the directional move algorithm**: given a focused tab and a direction (left/right/up/down), determine whether the tab moves to an existing neighbor pane or triggers a split, and how the tree is mutated in each case.
3. **Define the layout algorithm**: how the tree maps to screen rectangles, including support for both horizontal and vertical splits with proportional sizing.
4. **Define pane lifecycle**: what happens when a pane's last tab is moved out or closed — how the tree collapses empty nodes.
5. **Define focus navigation**: how `Cmd+Option+Arrow` (or similar) moves focus between panes without moving tabs.
6. **Identify integration points**: how the pane tree interacts with the existing `Workspace` model, `EditorState`, rendering, and input routing.
7. **Produce proposed chunks** that can be implemented incrementally.

## Testable Hypotheses

### H1: A binary split tree (each internal node has exactly two children and a split direction) is sufficient for the tiling model

- **Rationale**: Tiling window managers like bspwm use binary trees. i3/Sway use n-ary trees (each split node has N children). A binary tree is simpler for directional operations because "move right" always means "go to the other child of the nearest horizontal-split ancestor." An n-ary tree allows more than two children per split, which is more flexible but makes directional targeting ambiguous (which of n-1 siblings is "right"?).
- **Test**: Walk through the user's described scenario with both models and compare complexity. Check if n-ary trees produce ambiguous targets for directional moves.
- **Status**: UNTESTED

### H2: The directional move operation can be decomposed into "find target" and "execute move" phases with clear tree-walk algorithms

- **Rationale**: The move operation has two possible outcomes: (a) tab moves to an existing adjacent pane, or (b) current pane's parent node splits to create a new pane. If we can precisely define "adjacent pane in direction D" via a tree walk, and "split to create pane in direction D" via a tree mutation, the operation becomes mechanical.
- **Test**: Define the tree-walk algorithm for finding the target pane in each direction. Verify it handles edge cases: root pane (no parent), deeply nested trees, mixed split directions.
- **Status**: UNTESTED

### H3: The existing `Workspace` model (which owns tabs in a flat `Vec<Tab>`) can be refactored to delegate tab ownership to panes without breaking the rest of the editor

- **Rationale**: Currently `Workspace.tabs` owns all tabs and `Workspace.active_tab` is a single index. The pane model needs each pane to own its own tabs. The question is how much of `EditorState` and the rendering pipeline depends on `Workspace.tabs` directly.
- **Test**: Grep for all accesses to `Workspace.tabs`, `workspace.active_tab()`, `workspace.active_tab_mut()` and catalog the integration surface.
- **Status**: UNTESTED

### H4: Proportional sizing (each split stores a ratio like 0.5) combined with the tree structure produces a natural layout that handles window resizes gracefully

- **Rationale**: Tiling WMs typically store a split ratio per internal node rather than absolute pixel sizes. On resize, ratios are preserved and pixel sizes recompute. This is simpler than tracking absolute sizes and reflowing.
- **Test**: Sketch the layout algorithm with ratios and verify it handles resize, deeply nested splits, and minimum pane size constraints.
- **Status**: UNTESTED

### H5: Empty pane cleanup (collapsing the tree when a pane loses its last tab) can be done as a post-operation fixup without complicating the move/close logic

- **Rationale**: After moving a tab out of a pane, if the pane is empty, the tree node and its parent split should collapse. If this is a separate "cleanup" pass after every operation, the move/close logic stays simple — it just does the move, then cleanup runs.
- **Test**: Enumerate the cases: single pane left after move, deeply nested empty pane, sibling promotion when parent split has one child. Verify cleanup handles all without leaving the tree in an invalid state.
- **Status**: UNTESTED

## Exploration Log

### 2026-02-22: Cataloging current architecture and integration surface

**Goal**: Understand how deeply the flat-tab model is embedded in the editor before designing the pane tree.

#### Current data model

The hierarchy today is:

```
Editor
  └── Vec<Workspace>
        ├── tabs: Vec<Tab>          ← flat list of tabs
        ├── active_tab: usize       ← single active tab index
        ├── tab_bar_view_offset     ← scroll state for tab bar
        └── agent: Option<AgentHandle>
```

#### Integration surface for Workspace.tabs

The `EditorState` uses delegate methods that chain through:
```
editor.active_workspace() → workspace.active_tab() → tab.as_text_buffer()
```

Key access patterns:
- `EditorState::buffer()` / `buffer_mut()` — gets the active tab's TextBuffer
- `EditorState::viewport()` / `viewport_mut()` — gets the active tab's Viewport
- `EditorState::try_buffer()` — safe version for terminal tabs
- Tab creation: `Workspace::add_tab()`
- Tab closing: `Workspace::close_tab()`
- Tab switching: `Workspace::switch_tab()`
- Tab cycling: `EditorState::next_tab()` / `prev_tab()`
- Terminal polling: `Workspace::poll_standalone_terminals()` iterates `self.tabs`

The rendering pipeline accesses the active tab's buffer view via `Editor::active_buffer_view()`.

**Assessment**: The integration surface is moderate — roughly 15-20 call sites that assume `Workspace.tabs` is a flat list. The delegate methods in `EditorState` provide a clean interception layer: if the workspace model changes to use a pane tree, only the delegates need updating (they'd resolve `active_workspace → active_pane → active_tab` instead of `active_workspace → active_tab`).

### 2026-02-22: H1 — Binary vs N-ary split tree

**Goal**: Determine whether a binary tree or n-ary tree is the right model.

#### How tiling WMs model this

**bspwm (Binary Space Partitioning WM)**:
- Strict binary tree. Each internal node is a split (horizontal or vertical) with exactly two children.
- "Move right" from a node walks up until finding a horizontal split ancestor, then goes to the right child's leftmost leaf.
- Simple, unambiguous directional targeting.
- Limitation: creating a third column requires nesting (split right, then split the right half right again), producing unequal sizes unless ratios are adjusted.

**i3/Sway (N-ary tree)**:
- Each container (internal node) has a layout (horizontal/vertical/tabbed/stacking) and N children.
- "Move right" in a horizontal container moves to the next sibling.
- More intuitive for "I want three equal columns" (one horizontal container with three children).
- Complexity: directional moves must handle the case where "right" might mean "next sibling in this container" or "right child of a parent container."

**Hyprland**:
- Uses a binary tree (like bspwm) but with "groups" for tabbed containers.

#### Walking through the user scenario

**Scenario**: Start with one pane. Create tab B. Move tab B right. Create tab C in the right pane. Move tab C down.

**Binary tree model**:
```
Step 0: Pane[A, B]                      (single leaf)
Step 1: HSplit(Pane[A], Pane[B])         (Cmd+Shift+→ splits root horizontally)
Step 2: HSplit(Pane[A], Pane[B, C])      (Cmd+T adds tab C to right pane)
Step 3: HSplit(Pane[A], VSplit(Pane[B], Pane[C]))  (Cmd+Shift+↓ splits right child vertically)
```

Layout:
```
┌─────────┬─────────┐
│         │  Pane B  │
│ Pane A  ├─────────┤
│         │  Pane C  │
└─────────┴─────────┘
```

This works cleanly. Each directional move creates a new binary split at the current pane's position in the tree.

**N-ary tree model**: Same result, but the horizontal split could have 3+ children if the user moves another tab right from pane A. With binary trees, that would nest: `HSplit(HSplit(Pane[A], Pane[D]), VSplit(Pane[B], Pane[C]))`.

#### Analysis

For the described interaction model, **binary trees are the right choice** because:

1. **Directional moves have clear semantics**: "Move right" always means "I want this tab in a pane to the right of where I am." With a binary tree, this either moves to the sibling (if the parent is a horizontal split and the sibling is to the right) or creates a new horizontal split.

2. **No ambiguity**: With n-ary trees, moving right in a 3-child horizontal container is ambiguous — do you move to the immediately adjacent sibling, or to any sibling to the right?

3. **Tree mutation is simpler**: Splitting a binary tree node replaces the leaf with `Split(old_leaf, new_leaf)`. With n-ary, you need to decide whether to add a child to the current container or create a new nested container.

4. **N-ary splits add complexity without benefit for this interaction model**: Multiple horizontal children make directional targeting ambiguous. The binary model handles three-or-more columns through nesting, which maps naturally to the user's incremental splitting workflow.

**Conclusion**: H1 is **likely correct**. Binary split tree with each internal node having exactly two children and a `SplitDirection` (Horizontal or Vertical). Will verify fully after exploring H2.

**Status**: VERIFIED (pending H2 confirmation)

### 2026-02-22: H2 — Directional move algorithm design

**Goal**: Define precise algorithms for "find target pane in direction D" and "move tab to direction D."

#### Definitions

- **Split direction**: `Horizontal` means children are side-by-side (left/right). `Vertical` means children are stacked (top/bottom).
- **Child position**: In a split, one child is `First` and the other is `Second`. For Horizontal: First=left, Second=right. For Vertical: First=top, Second=bottom.
- **Move direction**: Left, Right, Up, Down.

#### Algorithm: Move tab in direction D

The user presses `Cmd+Shift+Arrow`. The focused pane has a tab that should move in direction D.

**Step 1: Find target**

Walk up the tree from the focused pane's leaf, looking for the nearest ancestor split whose direction is **compatible** with the move direction:

- Moving Right or Left → look for a `Horizontal` split ancestor
- Moving Up or Down → look for a `Vertical` split ancestor

For a compatible split:
- If the focused pane is in the `First` child and direction is Right/Down (toward Second) → the target is the `Second` child's nearest leaf in the opposite direction (leftmost for Right, topmost for Down).
- If the focused pane is in the `Second` child and direction is Left/Up (toward First) → the target is the `First` child's nearest leaf in the opposite direction.
- If the focused pane is already on the "far side" (e.g., in Second child and moving Right), continue walking up to find a higher compatible ancestor.

**If no compatible ancestor exists** (we walked all the way to the root without finding one), this is a **split-at-root** case: the root is replaced with a new split node.

**Step 2: Execute move**

Two outcomes:

**Case A — Target pane found**: Remove the tab from the source pane. Add it to the target pane. Focus moves to the target pane.

**Case B — No target, split needed**: Remove the tab from the source pane. Create a new pane containing just this tab. Replace the current leaf (or the root) with a new split node: `Split(direction, old_content, new_pane)` where the new pane goes in the direction of movement (Second for Right/Down, First for Left/Up).

**Wait — this needs more nuance.** Let me re-read the user's description:

> "Each motion either moves the tab to what is visually the target, if that is unambiguous, or splits the current node of the pane tree to include a new pane."

So the algorithm is:

1. **Is there an unambiguous visual target?** A pane that is directly adjacent in the given direction. If so, move the tab there.
2. **Otherwise, split.** The current pane's position in the tree gets a new sibling in the given direction.

This is actually simpler than my initial tree-walk. Let me redefine:

#### Revised algorithm: "Visual target or split"

**Finding the visual target**: Given screen rectangles for all panes, the visual target in direction D from pane P is a pane Q such that:
- Q's rectangle is adjacent to P's rectangle in direction D (shares an edge)
- Q is the **unique** such pane, or there's only one pane whose center aligns with P's center along the perpendicular axis

Actually this gets complicated geometrically. Let me think about it from the tree perspective instead.

#### Tree-based algorithm (cleaner)

**Move Right from pane P:**

1. Walk up from P to find the nearest `Horizontal` split ancestor A where P is in A's `First` subtree.
2. If found: the target is the leftmost leaf of A's `Second` subtree. Move tab there.
3. If not found (P is in the `Second` subtree of every horizontal ancestor, or there are no horizontal ancestors): **split P's parent.** Replace P's leaf with `Horizontal(P, NewPane)`. The new pane is to the right.

Wait, but what if P is already in the Second subtree of a Horizontal? Then "right" should either:
- Go to a higher horizontal ancestor's Second child (if P is in the First subtree of that one), or
- Split P itself.

Let me trace through a concrete example:

```
HSplit(Pane[A], VSplit(Pane[B], Pane[C]))
```

Moving Right from Pane C:
1. C's parent is VSplit. Direction is Horizontal, parent is Vertical → not compatible, continue up.
2. VSplit's parent is HSplit. Direction is Horizontal → compatible! Is VSplit (containing C) the First or Second child of HSplit? It's the Second child.
3. C is in the Second subtree of this Horizontal split, and we want to move Right (toward Second). So there's nowhere further right to go at this level. Continue up.
4. HSplit is the root. No more ancestors. → **Split case.**

What split should happen? The user is in pane C and pressing Right. They want C's tab to appear to the right of C. So we split C: replace `Pane[C]` with `HSplit(Pane[C_remaining], Pane[NewPane_with_moved_tab])`.

But wait — if C only has one tab and we're moving it, C becomes empty. So actually we'd replace the VSplit:

Hmm, this is getting into the details. Let me step back and define it more carefully.

#### Refined algorithm

The operation is: "Move the active tab of the focused pane in direction D."

**Preconditions**: The focused pane must have at least one tab (trivially true since we can't focus an empty pane). The tab being moved is the focused pane's active tab.

**Step 1: Find target using tree walk.**

```
fn find_target(node: &Tree, pane_id: PaneId, direction: Direction) -> MoveTarget {
    // Walk from pane to root, collecting the path
    let path = path_from_root_to(node, pane_id);
    
    // Walk up from the pane looking for a compatible split
    for ancestor in path.ancestors_from_pane() {
        if ancestor.split_direction.is_compatible(direction) {
            let which_child = ancestor.which_child_contains(pane_id);
            if direction.is_toward_second() && which_child == First {
                // Target is in Second subtree
                let target_leaf = ancestor.second.nearest_leaf_toward(direction.opposite());
                return MoveTarget::ExistingPane(target_leaf.pane_id);
            }
            if direction.is_toward_first() && which_child == Second {
                // Target is in First subtree
                let target_leaf = ancestor.first.nearest_leaf_toward(direction.opposite());
                return MoveTarget::ExistingPane(target_leaf.pane_id);
            }
            // Pane is on the "far side" — continue walking up
        }
    }
    
    // No compatible ancestor found — split the current pane
    MoveTarget::SplitPane(pane_id, direction)
}
```

**Step 2: Execute.**

For `ExistingPane(target_id)`:
- Remove active tab from source pane.
- Add tab to target pane.
- Focus target pane.
- If source pane is now empty, run cleanup.

For `SplitPane(pane_id, direction)`:
- Remove active tab from source pane.
- Create new pane with just this tab.
- Replace `Pane(source)` in the tree with `Split(direction_to_split_dir(direction), Pane(source), Pane(new))` where ordering depends on direction (Right/Down → source is First, new is Second; Left/Up → new is First, source is Second).
- Focus the new pane.
- If source pane is now empty (it had only one tab), the split immediately collapses: replace the split with just the new pane. **Actually** — if the source pane only had one tab, moving it out leaves an empty pane. The split we just created has an empty first child. Cleanup removes it, leaving just the new pane. Net effect: the tab "moved" but the pane structure is the same as before (just the tab is in a pane with the same position). This is a no-op! So we should detect this: **if the source pane has only one tab, a split-move is a no-op** (there's nothing to split from). In this case, the move should either be rejected or the entire pane should move (reparent it in the tree rather than moving just the tab).

Actually, re-reading the user's intent: "I press Command + Shift + Right Arrow to move the tab I'm on to a new pane that is on the right." If the pane has one tab and you move it right, you want to create a split where the tab goes right and... the left pane is empty? That doesn't make sense. The move should only be meaningful when:

1. **Multiple tabs in the source pane**: Moving one tab out creates a new pane for it, splitting the space.
2. **Single tab and a target exists**: Moving the single tab to an adjacent pane effectively merges/transfers.
3. **Single tab and no target**: This is a no-op (you can't split a single tab out of itself).

**This aligns with the user scenario**: "I create a new tab so that there are now more than one, and I press Command + Shift + Right Arrow" — explicitly noting that you need more than one tab for the split to make sense.

**Conclusion**: H2 is **verified**. The algorithm decomposes cleanly into find-target (tree walk) and execute-move (remove tab + insert into target or split). Edge cases (single-tab pane, root splitting) have clear handling.

**Status**: VERIFIED

### 2026-02-22: H3 — Integration surface assessment

**Goal**: Determine how much of the codebase depends on the flat `Workspace.tabs` model.

Key call sites that would need to change:

1. **`EditorState` delegate methods** (`buffer()`, `viewport()`, `try_buffer()`, etc.): These chain through `editor.active_workspace().active_tab()`. With panes, this becomes `editor.active_workspace().active_pane().active_tab()`. ~10 methods in `EditorState`.

2. **Tab creation** (`new_tab()`, `new_terminal_tab()`): Currently calls `workspace.add_tab()`. Would need to call `active_pane.add_tab()`.

3. **Tab closing** (`close_active_tab()`): Currently calls `workspace.close_tab()`. Would need `active_pane.close_tab()` + empty pane cleanup.

4. **Tab cycling** (`next_tab()`, `prev_tab()`): Currently cycles within `workspace.tabs`. Would cycle within `active_pane.tabs`.

5. **Tab bar rendering** (`tabs_from_workspace()`): Currently reads `workspace.tabs`. Would read `active_pane.tabs` (each pane has its own tab bar).

6. **Terminal polling** (`poll_standalone_terminals()`): Iterates `workspace.tabs`. Would need to iterate all panes' tabs.

7. **Renderer**: Currently renders one content area. Would need to render multiple pane rectangles, each with its own tab bar and content.

8. **Mouse/click handling**: Currently assumes a single content area. Would need hit-testing against pane rectangles to route clicks.

9. **Keybinding routing**: Currently all keys go to the single active tab. Would need pane focus to determine which pane receives input.

**Assessment**: The refactoring is tractable. The `EditorState` delegate pattern provides a clean interception point. The new `Pane` type will need to mirror `Workspace`'s tab-management API (`add_tab`, `close_tab`, `switch_tab`, `active_tab`). The biggest work items are renderer changes (rendering multiple panes) and input routing (pane focus).

**Status**: VERIFIED — integration surface is moderate and manageable with incremental chunks.

### 2026-02-22: H4 — Proportional sizing and layout algorithm

**Goal**: Verify that ratio-based sizing works for the pane tree.

#### Layout algorithm sketch

Each `Split` node stores a `ratio: f32` (default 0.5) representing the fraction of space allocated to the First child.

```rust
fn layout(node: &Node, rect: Rect) -> Vec<(PaneId, Rect)> {
    match node {
        Node::Leaf(pane) => vec![(pane.id, rect)],
        Node::Split { direction, ratio, first, second } => {
            let (r1, r2) = match direction {
                Horizontal => rect.split_horizontal(*ratio),
                Vertical => rect.split_vertical(*ratio),
            };
            let mut result = layout(first, r1);
            result.extend(layout(second, r2));
            result
        }
    }
}
```

Where `split_horizontal(ratio)` divides width and `split_vertical(ratio)` divides height.

**Resize behavior**: When the window resizes, re-run layout with the new root rect. All ratios are preserved. Panes scale proportionally.

**Minimum pane size**: During layout, enforce a minimum width/height. If a pane would be too small, clamp to minimum and steal space from its sibling. This prevents panes from becoming unusably narrow.

**Manual ratio adjustment**: Future feature — the user could drag dividers to adjust ratios. Each divider corresponds to a Split node's ratio. Not needed for the initial implementation (all splits start at 0.5).

**Status**: VERIFIED — straightforward recursive algorithm. Ratios handle resize naturally.

### 2026-02-22: H5 — Empty pane cleanup

**Goal**: Verify that tree cleanup after tab moves/closes is simple.

#### Cleanup rules

After any operation that removes a tab from a pane:

1. **If the pane still has tabs**: No cleanup needed.
2. **If the pane is empty**: The pane should be removed from the tree.
   - Its parent split node has two children. One is the empty pane, the other is the sibling.
   - **Replace the parent split with the sibling.** The sibling (whether a leaf pane or a nested split) takes the parent's position in the tree.
   - **Recursively check**: If the sibling promotion caused its new parent to also become trivial (a split with only one meaningful child), collapse again. In practice, this only happens if both children of a split become empty simultaneously, which can't happen from a single tab move.

3. **Special case — root becomes empty**: If the root is a pane and its last tab is closed, the workspace has no content. This is the "close last tab" case, handled by existing workspace logic (create a new empty tab, or close the workspace).

#### Example trace

```
Before: HSplit(Pane[A, B], VSplit(Pane[C], Pane[D]))
```

Move tab C to Pane A:
```
After move: HSplit(Pane[A, B, C], VSplit(Pane[], Pane[D]))
Cleanup: Pane[] is empty → promote sibling Pane[D] → HSplit(Pane[A, B, C], Pane[D])
```

Move tab D to Pane A:
```
After move: HSplit(Pane[A, B, C, D], Pane[])
Cleanup: Pane[] is empty → promote sibling Pane[A, B, C, D] → Pane[A, B, C, D] (back to single pane)
```

Clean and simple. The tree contracts naturally.

**Status**: VERIFIED — cleanup is a single-pass bottom-up fixup. Replace empty-pane's parent split with the non-empty sibling.

### 2026-02-22: Mouse coordinate routing — the historical pain point

**Goal**: Design mouse event routing for multiple panes that avoids the class of coordinate bugs that have plagued the single-pane editor.

#### The current problem

The editor has accumulated at least 7 bug-fix chunks for mouse coordinate issues: `click_cursor_rail_offset`, `click_scroll_fraction_alignment`, `resize_click_alignment`, `terminal_mouse_offset`, `tab_click_cursor_placement`, `selector_coord_flip`, `wrap_click_offset`. The root cause is a **layered ad-hoc coordinate transformation pipeline**:

1. macOS delivers `MouseEvent` in NSView coordinates (origin bottom-left, y increases upward).
2. `handle_mouse()` checks raw coordinates against `RAIL_WIDTH` and `TAB_BAR_HEIGHT` to determine which region was clicked.
3. `handle_mouse_buffer()` subtracts `RAIL_WIDTH` from x, passes a reduced `content_height` for y-flipping.
4. `pixel_to_buffer_position()` flips y using `view_height`, adds back `scroll_fraction_px`.
5. The terminal path does its own parallel version of the same math.

Each layer does its own partial transform. The y-flip happens in `pixel_to_buffer_position` (deep in the call stack), not at the entry point. Offsets like `RAIL_WIDTH` and `TAB_BAR_HEIGHT` are subtracted at different points depending on the code path. This makes it extremely easy to introduce off-by-one or double-subtraction bugs.

With multiple panes, each pane has a different origin and size. If each pane handler independently re-derives coordinates by subtracting its own offsets, the bug surface multiplies by the number of panes.

#### The solution: transform once, dispatch in screen space

The multi-pane mouse routing must follow a strict pipeline:

**Step 1: Flip y once at the entry point.**

```rust
fn handle_mouse(&mut self, raw_event: MouseEvent) {
    // Convert from NSView (bottom-left origin) to screen space (top-left origin) immediately.
    let screen_x = raw_event.position.0;
    let screen_y = self.view_height as f64 - raw_event.position.1;
    // From this point on, ALL code works in screen space. No more flipping.
}
```

**Step 2: Hit-test against layout regions in screen space.**

The layout produces `PaneRect` values in screen space (origin top-left). Hit-testing is a simple `point_in_rect`:

```rust
// Check left rail (screen space)
if screen_x < RAIL_WIDTH { handle_rail_click(...); return; }

// For each pane rect (from layout calculation):
for pane_rect in &pane_rects {
    // Check tab bar region within this pane
    if screen_y >= pane_rect.y && screen_y < pane_rect.y + TAB_BAR_HEIGHT {
        if screen_x >= pane_rect.x && screen_x < pane_rect.x + pane_rect.width {
            handle_pane_tab_bar_click(pane_rect.pane_id, ...);
            return;
        }
    }
    // Check content region within this pane
    let content_y = pane_rect.y + TAB_BAR_HEIGHT;
    let content_height = pane_rect.height - TAB_BAR_HEIGHT;
    if point_in_rect(screen_x, screen_y, pane_rect.x, content_y, pane_rect.width, content_height) {
        // Transform to pane-local coordinates
        let local_x = screen_x - pane_rect.x;
        let local_y = screen_y - content_y;
        handle_pane_content_click(pane_rect.pane_id, local_x, local_y, ...);
        return;
    }
}
```

**Step 3: Pane content handlers receive pane-local coordinates.**

`pixel_to_buffer_position` receives coordinates where `(0, 0)` is the top-left of the pane's content area and y increases downward. It never sees window coordinates, never needs to know about `RAIL_WIDTH`, other panes' positions, or the y-flip. The only adjustment it makes is for `scroll_fraction_px` (sub-line scroll offset), which is intrinsic to the pane's viewport.

```rust
fn pixel_to_buffer_position(local_x: f64, local_y: f64, ...) -> Position {
    // local_y is already in screen space (0 = top of this pane's content area)
    // No flip needed. Just divide by line_height and add scroll offset.
    let screen_line = ((local_y + scroll_fraction_px) / line_height).floor() as usize;
    let buffer_line = scroll_offset + screen_line;
    // ...
}
```

**Step 4: Set pane focus on click.**

Any click within a pane's rectangle (tab bar or content) also sets that pane as the focused pane. This is the natural way focus works in tiling WMs — click to focus.

#### Why this eliminates the historical bug class

- **The y-flip happens exactly once**, at the entry point of `handle_mouse`. No downstream code ever sees NSView coordinates.
- **No handler subtracts global offsets ad-hoc.** The pane-local transform is computed from the `PaneRect` at the dispatch point, not scattered across multiple functions.
- **`pixel_to_buffer_position` becomes a pure function of pane-local geometry.** It doesn't know about window layout, rail widths, or tab bar heights. It just maps `(local_x, local_y)` to a buffer position using font metrics and scroll state. Testable in isolation.
- **The terminal and file-buffer paths use the same coordinate pipeline.** They both receive pane-local coordinates. No parallel ad-hoc transform paths.

This is a prerequisite for the multi-pane rendering chunk and should be designed into the integration chunk (chunk 3) rather than bolted on later.

## Findings

### Verified Findings

- **Binary split tree is the right model.** Each internal node has exactly two children and a `SplitDirection` (Horizontal or Vertical). This gives unambiguous directional targeting and simpler tree mutations than n-ary trees.

- **Directional tab movement decomposes into find-target + execute.** The find-target phase walks up the tree from the focused pane looking for a compatible split ancestor. If found and the pane is on the correct side, the target is the nearest leaf in the other subtree. If not found, the operation creates a new split at the pane's position. The algorithm is a clean tree walk.

- **Single-tab pane moves are constrained.** Moving a tab out of a pane that only has one tab is only meaningful if there's an existing target pane. Splitting a single-tab pane creates an empty sibling, which immediately collapses — a no-op. The UI should either (a) prevent split-moves from single-tab panes, or (b) "move the whole pane" (reparent it in the tree) instead.

- **Integration surface is moderate and manageable.** The `EditorState` delegate methods provide a clean interception layer. ~10 delegate methods, tab creation/closing, tab cycling, tab bar rendering, terminal polling, renderer, and input routing need updates.

- **Ratio-based proportional sizing works.** Each split stores a ratio (default 0.5). Layout is a recursive function that splits rectangles. Window resizes preserve ratios automatically. Minimum pane sizes can be enforced as a clamp.

- **Empty pane cleanup is a simple post-operation fixup.** When a pane becomes empty, replace its parent split with the non-empty sibling. The tree contracts naturally. No complex bookkeeping needed.

- **Mouse coordinate routing must flip y once at the entry point and transform to pane-local coordinates at the dispatch point.** The current editor has accumulated 7+ bug-fix chunks from ad-hoc coordinate transforms scattered across handlers. The multi-pane design must enforce a strict pipeline: (1) flip y from NSView to screen space once in `handle_mouse`, (2) hit-test against `PaneRect` values in screen space, (3) compute pane-local coordinates by subtracting the pane's origin at the dispatch point, (4) all downstream handlers (buffer hit-testing, terminal cell mapping) receive pane-local coordinates and never see window-global offsets. This makes `pixel_to_buffer_position` a pure function of pane-local geometry, eliminates the class of offset bugs, and means the terminal and file-buffer paths share the same coordinate pipeline.

### Hypotheses/Opinions

- **Focus navigation (Cmd+Option+Arrow to move between panes without moving tabs) should reuse the same tree-walk algorithm** as tab movement, just without the "move" step. Find the target pane in direction D, switch focus to it.

- **The pane tree should live inside Workspace**, replacing `tabs: Vec<Tab>` with something like `pane_root: PaneLayoutNode` plus `active_pane_id: PaneId`. Each workspace has its own pane tree.

- **Rendering multiple panes is the biggest implementation effort.** Each pane needs its own tab bar, content area, and scroll state. The renderer currently assumes a single content area. This will need a "render loop per pane" approach within each frame, with clip rectangles to constrain each pane's rendering.

- **A 1-pixel divider line between panes** (similar to tiling WMs) would provide clear visual separation. Future work could make dividers draggable for ratio adjustment.

- **Keybinding scheme**: `Cmd+Shift+Arrow` for tab movement, `Cmd+Option+Arrow` for focus movement. These don't conflict with existing bindings (`Cmd+Shift+[/]` for tab cycling, `Cmd+[/]` for workspace cycling).

## Proposed Chunks

The following chunks build the tiling pane system incrementally. Early chunks modify the data model and tree operations in isolation (testable without rendering). Later chunks wire the model into the editor.

1. **Binary pane tree data model**: Implement a binary pane layout tree: `Leaf(Pane)` | `Split { direction, ratio, first, second }`. Support both horizontal and vertical splits. Implement layout calculation (rect splitting with ratios). `Pane` owns its own tabs. Add tree traversal helpers: path-to-pane, find-target-in-direction, nearest-leaf. Comprehensive unit tests for layout and traversal.

2. **Directional tab movement operations**: Implement `move_tab(pane_id, direction)` on the tree: find-target via tree walk, execute move (remove from source, add to target or split). Implement empty-pane cleanup (promote sibling when pane becomes empty). Unit tests for all move scenarios: split creation, move to existing neighbor, tree collapse on empty.

3. **Workspace pane integration**: Replace `Workspace.tabs` / `Workspace.active_tab` with `Workspace.pane_root` / `Workspace.active_pane_id`. Update `EditorState` delegate methods to resolve through the active pane. Update tab creation, closing, and cycling to operate on the active pane. Update terminal polling to traverse all panes. Maintain backward compatibility for single-pane workspaces (a single Leaf node behaves identically to the old flat model). Refactor `handle_mouse` to flip y once at entry and transform to pane-local coordinates at the dispatch point — eliminating the ad-hoc coordinate transform pipeline that has caused 7+ bug-fix chunks historically.

4. **Multi-pane rendering**: Update the renderer to iterate pane rectangles and render each pane independently — tab bar, content area, cursor, selection. Add clip rectangles per pane. Render divider lines between panes. Handle the focused-pane visual indicator (subtle border or highlight). Each pane's content renderer receives pane-local geometry; `pixel_to_buffer_position` becomes a pure function that never sees window-global offsets.

5. **Pane focus navigation and keybindings**: Wire `Cmd+Shift+Arrow` to `move_tab(active_pane, direction)`. Wire `Cmd+Option+Arrow` (or `Ctrl+Arrow` — TBD) to focus-switch between panes. Mouse clicks within a pane's rectangle set that pane as focused. Hit-testing uses the `PaneRect` values computed by the layout algorithm in screen space — no per-handler offset math.

## Resolution Rationale

<!-- To be written when investigation reaches a terminal status. -->
