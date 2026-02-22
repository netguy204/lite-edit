// Chunk: docs/chunks/agent_lifecycle - Agent lifecycle integration tests
//! Integration tests for agent lifecycle management.
//!
//! These tests spawn real processes to verify end-to-end agent behavior.

use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use lite_edit_terminal::{AgentConfig, AgentHandle, AgentState, BufferView};

/// Creates a test config with short timeouts for faster tests.
fn test_config(command: &str, args: &[&str]) -> AgentConfig {
    AgentConfig {
        command: command.to_string(),
        args: args.iter().map(|s| s.to_string()).collect(),
        cwd: PathBuf::from("/tmp"),
        needs_input_timeout: Duration::from_millis(100),
        stale_timeout: Duration::from_millis(200),
    }
}

// =============================================================================
// State Transition Tests
// =============================================================================

#[test]
fn test_agent_starting_state() {
    // Spawn a command that produces output quickly
    let config = test_config("echo", &["hello"]);
    let agent = AgentHandle::spawn(config, 80, 24).unwrap();

    // Initial state should be Starting
    assert!(
        matches!(agent.state(), AgentState::Starting),
        "Expected Starting, got {:?}",
        agent.state()
    );
}

#[test]
fn test_agent_starting_to_running() {
    // Spawn a command that produces output
    let config = test_config("echo", &["hello"]);
    let mut agent = AgentHandle::spawn(config, 80, 24).unwrap();

    // Poll until we get output (state transitions to Running then Exited)
    for _ in 0..50 {
        agent.poll();
        thread::sleep(Duration::from_millis(10));

        match agent.state() {
            AgentState::Running | AgentState::Exited { .. } => {
                // Success - we transitioned through Running
                return;
            }
            _ => continue,
        }
    }

    // echo is fast, should have reached Running or Exited by now
    panic!(
        "Expected to reach Running or Exited, but state is {:?}",
        agent.state()
    );
}

#[test]
fn test_agent_exited_with_code_0() {
    // Spawn a command that exits successfully
    let config = test_config("true", &[]);
    let mut agent = AgentHandle::spawn(config, 80, 24).unwrap();

    // Poll until exited
    for _ in 0..50 {
        agent.poll();
        thread::sleep(Duration::from_millis(10));

        if matches!(agent.state(), AgentState::Exited { .. }) {
            break;
        }
    }

    assert!(
        matches!(agent.state(), AgentState::Exited { code: 0 }),
        "Expected Exited(0), got {:?}",
        agent.state()
    );
}

#[test]
fn test_agent_exited_with_nonzero_code() {
    // Spawn a command that exits with error
    let config = test_config("false", &[]);
    let mut agent = AgentHandle::spawn(config, 80, 24).unwrap();

    // Poll until exited
    for _ in 0..50 {
        agent.poll();
        thread::sleep(Duration::from_millis(10));

        if matches!(agent.state(), AgentState::Exited { .. }) {
            break;
        }
    }

    match agent.state() {
        AgentState::Exited { code } => {
            assert_ne!(*code, 0, "Expected non-zero exit code");
        }
        other => panic!("Expected Exited, got {:?}", other),
    }
}

#[test]
fn test_agent_needs_input_after_idle() {
    // Spawn cat, which waits for input and produces no output
    let config = test_config("cat", &[]);
    let mut agent = AgentHandle::spawn(config, 80, 24).unwrap();

    // Wait for needs_input_timeout (100ms) plus some margin
    thread::sleep(Duration::from_millis(200));
    agent.poll();

    // Should transition to NeedsInput (cat produces no output)
    assert!(
        matches!(
            agent.state(),
            AgentState::NeedsInput { .. } | AgentState::Stale { .. }
        ),
        "Expected NeedsInput or Stale, got {:?}",
        agent.state()
    );

    // Stop the agent to clean up
    let _ = agent.stop();
}

#[test]
fn test_agent_stale_after_long_idle() {
    // Spawn cat with very short timeouts
    let mut config = test_config("cat", &[]);
    config.needs_input_timeout = Duration::from_millis(50);
    config.stale_timeout = Duration::from_millis(100);

    let mut agent = AgentHandle::spawn(config, 80, 24).unwrap();

    // Poll multiple times to allow state transitions
    // Starting → NeedsInput (after 50ms) → Stale (after 100ms more)
    for _ in 0..30 {
        thread::sleep(Duration::from_millis(20));
        agent.poll();

        if matches!(agent.state(), AgentState::Stale { .. }) {
            break;
        }
    }

    // Should be Stale
    assert!(
        matches!(agent.state(), AgentState::Stale { .. }),
        "Expected Stale, got {:?}",
        agent.state()
    );

    // Stop the agent to clean up
    let _ = agent.stop();
}

// =============================================================================
// Restart Tests
// =============================================================================

#[test]
fn test_agent_restart_from_exited() {
    // Spawn a quick command
    let config = test_config("true", &[]);
    let mut agent = AgentHandle::spawn(config, 80, 24).unwrap();

    // Wait for exit
    for _ in 0..50 {
        agent.poll();
        thread::sleep(Duration::from_millis(10));
        if matches!(agent.state(), AgentState::Exited { .. }) {
            break;
        }
    }

    assert!(agent.can_restart());

    // Restart
    agent.restart().unwrap();

    // Should be back to Starting
    assert!(
        matches!(agent.state(), AgentState::Starting),
        "Expected Starting after restart, got {:?}",
        agent.state()
    );
}

#[test]
fn test_agent_cannot_restart_when_running() {
    // Spawn cat (stays running)
    let config = test_config("cat", &[]);
    let mut agent = AgentHandle::spawn(config, 80, 24).unwrap();

    assert!(!agent.can_restart());

    // Clean up
    let _ = agent.stop();
}

// =============================================================================
// Stop Tests
// =============================================================================

#[test]
fn test_agent_stop() {
    // Spawn cat (stays running)
    let config = test_config("cat", &[]);
    let mut agent = AgentHandle::spawn(config, 80, 24).unwrap();

    // Stop should work
    agent.stop().unwrap();

    // Should be Exited after stop
    assert!(
        matches!(agent.state(), AgentState::Exited { .. }),
        "Expected Exited after stop, got {:?}",
        agent.state()
    );
}

#[test]
fn test_agent_stop_when_already_exited_fails() {
    // Spawn a quick command
    let config = test_config("true", &[]);
    let mut agent = AgentHandle::spawn(config, 80, 24).unwrap();

    // Wait for exit
    for _ in 0..50 {
        agent.poll();
        thread::sleep(Duration::from_millis(10));
        if matches!(agent.state(), AgentState::Exited { .. }) {
            break;
        }
    }

    // Stop should fail since already exited
    assert!(agent.stop().is_err());
}

// =============================================================================
// Terminal Integration Tests
// =============================================================================

#[test]
fn test_agent_terminal_has_output() {
    // Spawn echo to get some output
    let config = test_config("echo", &["test output"]);
    let mut agent = AgentHandle::spawn(config, 80, 24).unwrap();

    // Poll until we have output
    for _ in 0..50 {
        agent.poll();
        thread::sleep(Duration::from_millis(10));
    }

    // Terminal should have some content
    let terminal = agent.terminal();
    // The terminal should have at least one line
    assert!(terminal.line_count() > 0);
}

#[test]
fn test_agent_write_input() {
    // Spawn cat
    let config = test_config("cat", &[]);
    let mut agent = AgentHandle::spawn(config, 80, 24).unwrap();

    // Write some input
    let result = agent.write_input(b"hello\n");
    assert!(result.is_ok());

    // Poll to process
    thread::sleep(Duration::from_millis(50));
    agent.poll();

    // Stop the agent
    let _ = agent.stop();
}

#[test]
fn test_agent_resize() {
    let config = test_config("cat", &[]);
    let mut agent = AgentHandle::spawn(config, 80, 24).unwrap();

    // Resize should work
    agent.resize(120, 40);

    // Terminal size should be updated
    assert_eq!(agent.terminal().size(), (120, 40));

    // Stop the agent
    let _ = agent.stop();
}

// =============================================================================
// Config Tests
// =============================================================================

#[test]
fn test_agent_config_builder() {
    let config = AgentConfig::new("claude")
        .with_args(vec!["--model".into(), "opus".into()])
        .with_cwd(PathBuf::from("/home/user"))
        .with_needs_input_timeout(Duration::from_secs(10))
        .with_stale_timeout(Duration::from_secs(120));

    assert_eq!(config.command, "claude");
    assert_eq!(config.args, vec!["--model", "opus"]);
    assert_eq!(config.cwd, PathBuf::from("/home/user"));
    assert_eq!(config.needs_input_timeout, Duration::from_secs(10));
    assert_eq!(config.stale_timeout, Duration::from_secs(120));
}
