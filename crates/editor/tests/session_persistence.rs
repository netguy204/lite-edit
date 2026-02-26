// Chunk: docs/chunks/workspace_session_persistence - Integration test for session persistence
//!
//! Integration tests for session file persistence.
//!
//! These tests verify end-to-end save/restore functionality by:
//! 1. Creating an Editor with workspaces and tabs
//! 2. Saving the session to a temp directory
//! 3. Loading the session back
//! 4. Verifying the restored structure matches the original

use std::fs;
use tempfile::TempDir;

use lite_edit::workspace::{Editor, Tab};
use lite_edit::session::{
    SessionData, WorkspaceData, PaneLayoutData, PaneData, TabData, SplitDirectionData,
};
use lite_edit_buffer::TextBuffer;

const TEST_LINE_HEIGHT: f32 = 16.0;

/// Tests that an Editor with multiple workspaces and tabs round-trips through
/// save/restore correctly.
#[test]
fn test_full_session_roundtrip() {
    // Create two temp directories to serve as workspace roots
    let temp1 = TempDir::new().unwrap();
    let temp2 = TempDir::new().unwrap();
    let root1 = temp1.path().to_path_buf();
    let root2 = temp2.path().to_path_buf();

    // Create test files in each workspace
    let file1a = root1.join("file_a.rs");
    let file1b = root1.join("file_b.rs");
    let file2a = root2.join("main.rs");
    fs::write(&file1a, "// File A content").unwrap();
    fs::write(&file1b, "// File B content").unwrap();
    fs::write(&file2a, "fn main() {}").unwrap();

    // Create an Editor with two workspaces
    let mut editor = Editor::new_deferred(TEST_LINE_HEIGHT);

    // First workspace with two file tabs
    editor.new_workspace("Workspace 1".to_string(), root1.clone());
    {
        // Generate tab IDs before borrowing workspace mutably
        let tab_id1 = editor.gen_tab_id();
        let tab_id2 = editor.gen_tab_id();

        let ws = editor.active_workspace_mut().unwrap();

        // Add first file tab
        let buffer = TextBuffer::from_str("// File A content");
        let tab = Tab::new_file(tab_id1, buffer, "file_a.rs".to_string(), Some(file1a.clone()), TEST_LINE_HEIGHT);
        ws.add_tab(tab);

        // Add second file tab
        let buffer = TextBuffer::from_str("// File B content");
        let tab = Tab::new_file(tab_id2, buffer, "file_b.rs".to_string(), Some(file1b.clone()), TEST_LINE_HEIGHT);
        ws.add_tab(tab);
    }

    // Second workspace with one file tab
    editor.new_workspace("Workspace 2".to_string(), root2.clone());
    {
        let tab_id = editor.gen_tab_id();
        let ws = editor.active_workspace_mut().unwrap();

        let buffer = TextBuffer::from_str("fn main() {}");
        let tab = Tab::new_file(tab_id, buffer, "main.rs".to_string(), Some(file2a.clone()), TEST_LINE_HEIGHT);
        ws.add_tab(tab);
    }

    // Set workspace 1 as active
    editor.switch_workspace(0);

    // Verify pre-save state
    assert_eq!(editor.workspace_count(), 2);
    assert_eq!(editor.active_workspace, 0);

    // Convert to session data
    let session_data = SessionData::from_editor(&editor);

    // Verify session data structure
    assert_eq!(session_data.workspaces.len(), 2);
    assert_eq!(session_data.active_workspace, 0);
    assert_eq!(session_data.workspaces[0].root_path, root1);
    assert_eq!(session_data.workspaces[0].label, "Workspace 1");
    assert_eq!(session_data.workspaces[1].root_path, root2);
    assert_eq!(session_data.workspaces[1].label, "Workspace 2");

    // Serialize and deserialize JSON
    let json = serde_json::to_string_pretty(&session_data).unwrap();
    let restored_session: SessionData = serde_json::from_str(&json).unwrap();

    // Restore into a new editor
    let restored_editor = restored_session.restore_into_editor(TEST_LINE_HEIGHT).unwrap();

    // Verify restored state matches original
    assert_eq!(restored_editor.workspace_count(), 2);
    assert_eq!(restored_editor.active_workspace, 0);

    // Check workspace 1
    let ws1 = &restored_editor.workspaces[0];
    assert_eq!(ws1.root_path, root1);
    assert_eq!(ws1.label, "Workspace 1");
    // Should have 2 file tabs (plus the initial empty tab that gets replaced)
    // Actually it should have 3: initial empty + 2 file tabs = 3 tabs in session,
    // but the empty tab has no associated_file so it's not saved. So we should have 2.
    // Wait - the session only saves file tabs with associated files.
    // The original editor had 3 tabs in workspace 1 (1 empty + 2 file), but only 2 are saved.
    // On restore, those 2 files are loaded.
    assert_eq!(ws1.total_tab_count(), 2);

    // Check workspace 2
    let ws2 = &restored_editor.workspaces[1];
    assert_eq!(ws2.root_path, root2);
    assert_eq!(ws2.label, "Workspace 2");
    // Workspace 2: original had 2 tabs (1 empty + 1 file), session saves only the file tab
    assert_eq!(ws2.total_tab_count(), 1);
}

/// Tests that session persistence correctly handles split pane layouts.
#[test]
fn test_split_layout_persistence() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();

    // Create test files
    let file1 = root.join("left.rs");
    let file2 = root.join("right.rs");
    fs::write(&file1, "// Left pane").unwrap();
    fs::write(&file2, "// Right pane").unwrap();

    // Create session data with a split layout
    let session = SessionData {
        schema_version: 1,
        active_workspace: 0,
        workspaces: vec![WorkspaceData {
            root_path: root.clone(),
            label: "Split Test".to_string(),
            active_pane_id: 1,
            pane_root: PaneLayoutData::Split {
                direction: SplitDirectionData::Horizontal,
                ratio: 0.5,
                first: Box::new(PaneLayoutData::Leaf(PaneData {
                    id: 1,
                    tabs: vec![TabData { file_path: file1.clone() }],
                    active_tab: 0,
                })),
                second: Box::new(PaneLayoutData::Leaf(PaneData {
                    id: 2,
                    tabs: vec![TabData { file_path: file2.clone() }],
                    active_tab: 0,
                })),
            },
        }],
    };

    // Serialize and deserialize
    let json = serde_json::to_string(&session).unwrap();
    let restored: SessionData = serde_json::from_str(&json).unwrap();

    // Restore into editor
    let editor = restored.restore_into_editor(TEST_LINE_HEIGHT).unwrap();

    // Verify split layout
    let ws = editor.active_workspace().unwrap();
    assert_eq!(ws.pane_root.pane_count(), 2);

    // Active pane should be pane 1
    assert_eq!(ws.active_pane_id, 1);

    // Both panes should have one tab each
    let pane1 = ws.pane_root.get_pane(1).unwrap();
    let pane2 = ws.pane_root.get_pane(2).unwrap();
    assert_eq!(pane1.tab_count(), 1);
    assert_eq!(pane2.tab_count(), 1);
}

/// Tests graceful degradation when some workspaces or files are missing.
#[test]
fn test_partial_restoration() {
    // Create one temp directory and reference another that doesn't exist
    let temp = TempDir::new().unwrap();
    let valid_root = temp.path().to_path_buf();
    let invalid_root = std::path::PathBuf::from("/nonexistent/workspace/path");

    // Create a file in the valid workspace
    let valid_file = valid_root.join("exists.rs");
    fs::write(&valid_file, "// Valid file").unwrap();

    // Reference a file that doesn't exist
    let missing_file = valid_root.join("missing.rs");

    let session = SessionData {
        schema_version: 1,
        active_workspace: 0,
        workspaces: vec![
            // First workspace: invalid root path (should be skipped)
            WorkspaceData {
                root_path: invalid_root,
                label: "Invalid".to_string(),
                active_pane_id: 0,
                pane_root: PaneLayoutData::Leaf(PaneData {
                    id: 0,
                    tabs: vec![],
                    active_tab: 0,
                }),
            },
            // Second workspace: valid root, mixed valid/invalid files
            WorkspaceData {
                root_path: valid_root.clone(),
                label: "Valid".to_string(),
                active_pane_id: 0,
                pane_root: PaneLayoutData::Leaf(PaneData {
                    id: 0,
                    tabs: vec![
                        TabData { file_path: valid_file.clone() },
                        TabData { file_path: missing_file },
                    ],
                    active_tab: 0,
                }),
            },
        ],
    };

    let editor = session.restore_into_editor(TEST_LINE_HEIGHT).unwrap();

    // Only the valid workspace should be restored
    assert_eq!(editor.workspace_count(), 1);

    let ws = editor.active_workspace().unwrap();
    assert_eq!(ws.root_path, valid_root);
    assert_eq!(ws.label, "Valid");

    // Only the existing file should be restored (missing file skipped)
    assert_eq!(ws.total_tab_count(), 1);
    let tab = ws.active_tab().unwrap();
    assert_eq!(tab.associated_file.as_ref(), Some(&valid_file));
}

/// Tests that the active workspace index is preserved correctly.
#[test]
fn test_active_workspace_preservation() {
    let temp1 = TempDir::new().unwrap();
    let temp2 = TempDir::new().unwrap();
    let temp3 = TempDir::new().unwrap();

    let session = SessionData {
        schema_version: 1,
        active_workspace: 2, // Third workspace is active
        workspaces: vec![
            WorkspaceData {
                root_path: temp1.path().to_path_buf(),
                label: "First".to_string(),
                active_pane_id: 0,
                pane_root: PaneLayoutData::Leaf(PaneData {
                    id: 0,
                    tabs: vec![],
                    active_tab: 0,
                }),
            },
            WorkspaceData {
                root_path: temp2.path().to_path_buf(),
                label: "Second".to_string(),
                active_pane_id: 0,
                pane_root: PaneLayoutData::Leaf(PaneData {
                    id: 0,
                    tabs: vec![],
                    active_tab: 0,
                }),
            },
            WorkspaceData {
                root_path: temp3.path().to_path_buf(),
                label: "Third".to_string(),
                active_pane_id: 0,
                pane_root: PaneLayoutData::Leaf(PaneData {
                    id: 0,
                    tabs: vec![],
                    active_tab: 0,
                }),
            },
        ],
    };

    let editor = session.restore_into_editor(TEST_LINE_HEIGHT).unwrap();

    assert_eq!(editor.workspace_count(), 3);
    assert_eq!(editor.active_workspace, 2);
    assert_eq!(editor.active_workspace().unwrap().label, "Third");
}

/// Tests that active tab within a pane is preserved.
#[test]
fn test_active_tab_preservation() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();

    // Create three files
    let file1 = root.join("first.rs");
    let file2 = root.join("second.rs");
    let file3 = root.join("third.rs");
    fs::write(&file1, "1").unwrap();
    fs::write(&file2, "2").unwrap();
    fs::write(&file3, "3").unwrap();

    let session = SessionData {
        schema_version: 1,
        active_workspace: 0,
        workspaces: vec![WorkspaceData {
            root_path: root.clone(),
            label: "Test".to_string(),
            active_pane_id: 0,
            pane_root: PaneLayoutData::Leaf(PaneData {
                id: 0,
                tabs: vec![
                    TabData { file_path: file1.clone() },
                    TabData { file_path: file2.clone() },
                    TabData { file_path: file3.clone() },
                ],
                active_tab: 1, // Second tab is active
            }),
        }],
    };

    let editor = session.restore_into_editor(TEST_LINE_HEIGHT).unwrap();

    let ws = editor.active_workspace().unwrap();
    let pane = ws.active_pane().unwrap();

    // Should have 3 tabs with the second one (index 1) active
    assert_eq!(pane.tab_count(), 3);
    assert_eq!(pane.active_tab, 1);

    // Verify the active tab is the second file
    let active_tab = pane.active_tab().unwrap();
    assert_eq!(active_tab.associated_file.as_ref(), Some(&file2));
}

/// Tests deeply nested split layouts.
#[test]
fn test_nested_split_persistence() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();

    // Create files for each pane
    let file_a = root.join("a.rs");
    let file_b = root.join("b.rs");
    let file_c = root.join("c.rs");
    fs::write(&file_a, "A").unwrap();
    fs::write(&file_b, "B").unwrap();
    fs::write(&file_c, "C").unwrap();

    // Create a nested layout: HSplit(Pane[A], VSplit(Pane[B], Pane[C]))
    let session = SessionData {
        schema_version: 1,
        active_workspace: 0,
        workspaces: vec![WorkspaceData {
            root_path: root.clone(),
            label: "Nested".to_string(),
            active_pane_id: 2, // Pane B is active
            pane_root: PaneLayoutData::Split {
                direction: SplitDirectionData::Horizontal,
                ratio: 0.5,
                first: Box::new(PaneLayoutData::Leaf(PaneData {
                    id: 1,
                    tabs: vec![TabData { file_path: file_a.clone() }],
                    active_tab: 0,
                })),
                second: Box::new(PaneLayoutData::Split {
                    direction: SplitDirectionData::Vertical,
                    ratio: 0.5,
                    first: Box::new(PaneLayoutData::Leaf(PaneData {
                        id: 2,
                        tabs: vec![TabData { file_path: file_b.clone() }],
                        active_tab: 0,
                    })),
                    second: Box::new(PaneLayoutData::Leaf(PaneData {
                        id: 3,
                        tabs: vec![TabData { file_path: file_c.clone() }],
                        active_tab: 0,
                    })),
                }),
            },
        }],
    };

    let editor = session.restore_into_editor(TEST_LINE_HEIGHT).unwrap();

    let ws = editor.active_workspace().unwrap();

    // Should have 3 panes
    assert_eq!(ws.pane_root.pane_count(), 3);

    // Active pane should be pane 2 (B)
    assert_eq!(ws.active_pane_id, 2);

    // Verify each pane has the correct file
    let pane_a = ws.pane_root.get_pane(1).unwrap();
    let pane_b = ws.pane_root.get_pane(2).unwrap();
    let pane_c = ws.pane_root.get_pane(3).unwrap();

    assert_eq!(pane_a.tabs[0].associated_file.as_ref(), Some(&file_a));
    assert_eq!(pane_b.tabs[0].associated_file.as_ref(), Some(&file_b));
    assert_eq!(pane_c.tabs[0].associated_file.as_ref(), Some(&file_c));
}

/// Tests that all workspaces being invalid results in an error.
#[test]
fn test_all_workspaces_invalid_returns_error() {
    let session = SessionData {
        schema_version: 1,
        active_workspace: 0,
        workspaces: vec![
            WorkspaceData {
                root_path: std::path::PathBuf::from("/nonexistent/path/1"),
                label: "Invalid 1".to_string(),
                active_pane_id: 0,
                pane_root: PaneLayoutData::Leaf(PaneData {
                    id: 0,
                    tabs: vec![],
                    active_tab: 0,
                }),
            },
            WorkspaceData {
                root_path: std::path::PathBuf::from("/nonexistent/path/2"),
                label: "Invalid 2".to_string(),
                active_pane_id: 0,
                pane_root: PaneLayoutData::Leaf(PaneData {
                    id: 0,
                    tabs: vec![],
                    active_tab: 0,
                }),
            },
        ],
    };

    let result = session.restore_into_editor(TEST_LINE_HEIGHT);
    assert!(result.is_err());
}

// Chunk: docs/chunks/pane_mirror_restore - Regression test for pane ID collision bug
/// Tests that session restore with empty pane (terminal-only filtered) doesn't cause ID collision.
///
/// This test verifies the fix for the pane mirroring bug where:
/// 1. A workspace has a split layout with pane IDs 1 and 2
/// 2. Pane 2 originally had only a terminal tab (filtered during serialization)
/// 3. On restore, pane 2 gets an empty "Untitled" placeholder
/// 4. Creating a new pane should get ID 3, NOT ID 1 (which would collide)
///
/// Without the fix, workspace.next_pane_id wasn't synced after restore, causing
/// new panes to get conflicting IDs and the pane mirroring bug.
#[test]
fn test_empty_pane_restore_no_id_collision() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();

    // Create a file for the left pane
    let file_left = root.join("left.rs");
    fs::write(&file_left, "// Left pane content").unwrap();

    // Create session with horizontal split:
    // - Pane 1 (left): has a file tab
    // - Pane 2 (right): empty (simulates terminal-only pane after filtering)
    let session = SessionData {
        schema_version: 1,
        active_workspace: 0,
        workspaces: vec![WorkspaceData {
            root_path: root.clone(),
            label: "Test".to_string(),
            active_pane_id: 1, // Left pane is focused
            pane_root: PaneLayoutData::Split {
                direction: SplitDirectionData::Horizontal,
                ratio: 0.5,
                first: Box::new(PaneLayoutData::Leaf(PaneData {
                    id: 1,
                    tabs: vec![TabData { file_path: file_left.clone() }],
                    active_tab: 0,
                })),
                second: Box::new(PaneLayoutData::Leaf(PaneData {
                    id: 2,
                    tabs: vec![], // Empty - simulates terminal-only pane
                    active_tab: 0,
                })),
            },
        }],
    };

    // Restore the session
    let mut editor = session.restore_into_editor(TEST_LINE_HEIGHT).unwrap();
    let ws = editor.active_workspace_mut().unwrap();

    // Verify the restored structure
    assert_eq!(ws.pane_root.pane_count(), 2);
    assert!(ws.pane_root.get_pane(1).is_some(), "Pane 1 should exist");
    assert!(ws.pane_root.get_pane(2).is_some(), "Pane 2 should exist");

    // Pane 2 should have an empty placeholder tab
    let pane2 = ws.pane_root.get_pane(2).unwrap();
    assert_eq!(pane2.tab_count(), 1, "Empty pane should get placeholder tab");
    assert_eq!(pane2.tabs[0].label, "Untitled");

    // KEY TEST: Generate a new pane ID and verify it doesn't collide
    // Before the fix, this would return 1 (collision with existing pane)
    // After the fix, this should return 3 (next available ID)
    let new_pane_id = ws.gen_pane_id();
    assert!(
        new_pane_id > 2,
        "New pane ID {} should be > 2 to avoid collision with restored panes 1 and 2",
        new_pane_id
    );

    // Verify no pane with the new ID exists yet
    assert!(
        ws.pane_root.get_pane(new_pane_id).is_none(),
        "Generated pane ID {} should be unique (not yet used)",
        new_pane_id
    );
}

/// Tests that creating new panes after session restore works correctly.
///
/// This is a more comprehensive test that verifies the full workflow:
/// restore a split layout, then use move_active_tab to create a third pane.
#[test]
fn test_create_pane_after_restore() {
    use lite_edit::pane_layout::Direction;

    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();

    // Create files
    let file1 = root.join("file1.rs");
    let file2 = root.join("file2.rs");
    fs::write(&file1, "// File 1").unwrap();
    fs::write(&file2, "// File 2").unwrap();

    // Create session with two panes, left pane has two tabs
    let session = SessionData {
        schema_version: 1,
        active_workspace: 0,
        workspaces: vec![WorkspaceData {
            root_path: root.clone(),
            label: "Test".to_string(),
            active_pane_id: 1,
            pane_root: PaneLayoutData::Split {
                direction: SplitDirectionData::Horizontal,
                ratio: 0.5,
                first: Box::new(PaneLayoutData::Leaf(PaneData {
                    id: 1,
                    tabs: vec![
                        TabData { file_path: file1.clone() },
                        TabData { file_path: file2.clone() },
                    ],
                    active_tab: 1, // Second tab active
                })),
                second: Box::new(PaneLayoutData::Leaf(PaneData {
                    id: 2,
                    tabs: vec![], // Empty pane
                    active_tab: 0,
                })),
            },
        }],
    };

    // Restore and verify initial state
    let mut editor = session.restore_into_editor(TEST_LINE_HEIGHT).unwrap();

    {
        let ws = editor.active_workspace().unwrap();
        assert_eq!(ws.pane_root.pane_count(), 2);
        assert_eq!(ws.active_pane_id, 1);

        let pane1 = ws.pane_root.get_pane(1).unwrap();
        assert_eq!(pane1.tab_count(), 2);
        assert_eq!(pane1.active_tab, 1);
    }

    // Move the active tab down to create a new pane via split
    {
        let ws = editor.active_workspace_mut().unwrap();
        let result = ws.move_active_tab(Direction::Down);

        // Should have created a new pane
        use lite_edit::pane_layout::MoveResult;
        match result {
            MoveResult::MovedToNew { new_pane_id, .. } => {
                // Key assertion: new pane ID should not conflict with 1 or 2
                assert!(
                    new_pane_id > 2,
                    "New pane ID {} should be > 2 (after fix for pane_mirror_restore)",
                    new_pane_id
                );

                // Verify the new pane exists and has the moved tab
                let new_pane = ws.pane_root.get_pane(new_pane_id).unwrap();
                assert_eq!(new_pane.tab_count(), 1);

                // Original pane should now have 1 tab
                let pane1 = ws.pane_root.get_pane(1).unwrap();
                assert_eq!(pane1.tab_count(), 1);
            }
            MoveResult::MovedToExisting { .. } => {
                // This is also valid if pane 2 exists in that direction
                // The important thing is no crash from ID collision
            }
            _ => panic!("Expected tab to be moved, got {:?}", result),
        }
    }
}
