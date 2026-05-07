use super::metadata::{Command, CommandScope, CommandSpec};
use super::registry::CommandRegistry;
use super::types::{
    CommandData, CommandEffect, CommandInvocation, CommandRequest, PackageInvocationRequest,
};
use crate::kernel::{BufferContent, BufferId, JobKind, Workspace};

#[test]
fn registry_indexes_commands_by_name() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::new("quit", "Quit zred", CommandScope::Global));

    assert!(registry.contains("quit"));
    assert_eq!(registry.get("quit").unwrap().summary(), "Quit zred");
}

#[test]
fn dispatch_creates_and_focuses_new_buffer() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "buffer.new",
        "Create a text buffer",
        CommandScope::Workspace,
        CommandSpec::buffer_new(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "buffer.new notes");

    assert_eq!(workspace.buffer_count(), 2);
    assert_eq!(workspace.current_buffer().name(), "notes");
    assert_eq!(
        result.data(),
        Some(&CommandData::BufferCreated {
            buffer_id: workspace.current_buffer().id()
        })
    );
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus("Created notes".to_string())]
    );
}

#[test]
fn dispatch_quit_returns_quit_outcome() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "quit",
        "Quit zred",
        CommandScope::Global,
        CommandSpec::quit(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "quit");

    assert_eq!(result.effects(), &[CommandEffect::Quit]);
}

#[test]
fn dispatch_help_lists_registered_commands() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "buffer.new",
        "Create a text buffer",
        CommandScope::Workspace,
        CommandSpec::buffer_new(),
    ));
    registry.register(Command::with_spec(
        "help",
        "Show command help",
        CommandScope::Global,
        CommandSpec::help(),
    ));
    registry.register(Command::with_spec(
        "pane.split.vertical",
        "Split the active pane vertically",
        CommandScope::Workspace,
        CommandSpec::split_pane_vertical(),
    ));
    registry.register(Command::with_spec(
        "pane.next",
        "Focus the next pane",
        CommandScope::Workspace,
        CommandSpec::focus_next_pane(),
    ));
    registry.register(Command::with_spec(
        "pane.prev",
        "Focus the previous pane",
        CommandScope::Workspace,
        CommandSpec::focus_previous_pane(),
    ));
    registry.register(Command::with_spec(
        "pane.left",
        "Focus the pane to the left",
        CommandScope::Workspace,
        CommandSpec::focus_pane_left(),
    ));
    registry.register(Command::with_spec(
        "pane.right",
        "Focus the pane to the right",
        CommandScope::Workspace,
        CommandSpec::focus_pane_right(),
    ));
    registry.register(Command::with_spec(
        "pane.up",
        "Focus the pane above",
        CommandScope::Workspace,
        CommandSpec::focus_pane_up(),
    ));
    registry.register(Command::with_spec(
        "pane.down",
        "Focus the pane below",
        CommandScope::Workspace,
        CommandSpec::focus_pane_down(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "help");

    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "Commands: buffer.new, help, pane.down, pane.left, pane.next, pane.prev, pane.right, pane.split.vertical, pane.up".to_string()
        )]
    );
}

#[test]
fn dispatch_help_for_command_uses_command_metadata() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "help",
        "Show command help",
        CommandScope::Global,
        CommandSpec::help(),
    ));
    registry.register(Command::with_spec(
        "buffer.new",
        "Create a text buffer",
        CommandScope::Workspace,
        CommandSpec::buffer_new(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "help buffer.new");

    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "Create a text buffer: Usage: :buffer.new <name>".to_string()
        )]
    );
}

#[test]
fn parse_lowers_buffer_new_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "buffer.new",
        "Create a text buffer",
        CommandScope::Workspace,
        CommandSpec::buffer_new(),
    ));

    let parsed = registry.parse("buffer.new notes");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::BufferNew {
            name: "notes".to_string()
        })
    );
}

#[test]
fn parse_eval_without_script_returns_usage_status() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "eval",
        "Evaluate Lua code",
        CommandScope::Workspace,
        CommandSpec::eval_lua(),
    ));

    let parsed = registry.parse("eval");

    assert_eq!(
        parsed,
        CommandRequest::Status("Usage: :eval <lua code>".to_string())
    );
}

#[test]
fn parse_package_run_lowers_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "package.run",
        "Run a Zacor package command",
        CommandScope::Workspace,
        CommandSpec::package_run(),
    ));

    let parsed = registry.parse("package.run echo default value=hello");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::PackageRun {
            package: "echo".to_string(),
            command: "default".to_string(),
            args: [("value".to_string(), "hello".to_string())]
                .into_iter()
                .collect(),
        })
    );
}

#[test]
fn parse_registered_placeholder_command_returns_registered_status() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::new(
        "inspect",
        "Inspect current buffer",
        CommandScope::Buffer,
    ));

    let parsed = registry.parse("inspect");

    assert_eq!(
        parsed,
        CommandRequest::Status("Command registered but not dispatched yet: :inspect".to_string())
    );
}

#[test]
fn parse_quit_with_extra_args_stays_unknown() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "quit",
        "Quit zred",
        CommandScope::Global,
        CommandSpec::quit(),
    ));

    let parsed = registry.parse("quit now");

    assert_eq!(
        parsed,
        CommandRequest::Status("Unknown command: :quit now".to_string())
    );
}

#[test]
fn dispatch_eval_returns_lua_effect() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "eval",
        "Evaluate Lua code",
        CommandScope::Workspace,
        CommandSpec::eval_lua(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "eval minibuffer.message('hi')");

    assert_eq!(
        result.effects(),
        &[CommandEffect::EvalLua(
            "minibuffer.message('hi')".to_string()
        )]
    );
}

#[test]
fn dispatch_split_pane_creates_new_active_pane() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "pane.split.vertical",
        "Split the active pane vertically",
        CommandScope::Workspace,
        CommandSpec::split_pane_vertical(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "pane.split.vertical");

    assert_eq!(workspace.pane_count(), 2);
    assert_eq!(workspace.active_pane_id(), crate::kernel::PaneId::new(2));
    assert_eq!(workspace.current_buffer().name(), "*scratch*");
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "Split pane vertically".to_string()
        )]
    );
}

#[test]
fn dispatch_pane_next_cycles_focus_to_another_pane() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "pane.next",
        "Focus the next pane",
        CommandScope::Workspace,
        CommandSpec::focus_next_pane(),
    ));
    let mut workspace = Workspace::new();
    workspace.split_active_pane(crate::kernel::SplitAxis::Vertical);

    let result = registry.dispatch(&mut workspace, "pane.next");

    assert_eq!(workspace.active_pane_id(), crate::kernel::PaneId::new(1));
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus("Focused pane 1".to_string())]
    );
}

#[test]
fn dispatch_pane_next_reports_when_no_other_pane_exists() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "pane.next",
        "Focus the next pane",
        CommandScope::Workspace,
        CommandSpec::focus_next_pane(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "pane.next");

    assert_eq!(workspace.active_pane_id(), crate::kernel::PaneId::new(1));
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "No other pane to focus".to_string()
        )]
    );
}

#[test]
fn dispatch_pane_right_and_down_follow_split_geometry() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "pane.right",
        "Focus the pane to the right",
        CommandScope::Workspace,
        CommandSpec::focus_pane_right(),
    ));
    registry.register(Command::with_spec(
        "pane.down",
        "Focus the pane below",
        CommandScope::Workspace,
        CommandSpec::focus_pane_down(),
    ));
    let mut workspace = Workspace::new();
    workspace.split_active_pane(crate::kernel::SplitAxis::Vertical);
    workspace.split_active_pane(crate::kernel::SplitAxis::Horizontal);
    workspace.focus_previous_pane();
    workspace.focus_previous_pane();

    let right = registry.dispatch(&mut workspace, "pane.right");
    assert_eq!(workspace.active_pane_id(), crate::kernel::PaneId::new(2));
    assert_eq!(
        right.effects(),
        &[CommandEffect::SetStatus("Focused pane 2".to_string())]
    );

    let down = registry.dispatch(&mut workspace, "pane.down");
    assert_eq!(workspace.active_pane_id(), crate::kernel::PaneId::new(3));
    assert_eq!(
        down.effects(),
        &[CommandEffect::SetStatus("Focused pane 3".to_string())]
    );
}

#[test]
fn dispatch_pane_resize_left_updates_split_ratio() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "pane.resize.left",
        "Grow the active pane to the left",
        CommandScope::Workspace,
        CommandSpec::resize_pane_left(),
    ));
    let mut workspace = Workspace::new();
    workspace.split_active_pane(crate::kernel::SplitAxis::Vertical);

    let result = registry.dispatch(&mut workspace, "pane.resize.left");

    assert!(matches!(
        workspace.pane_tree().root(),
        crate::kernel::PaneNode::Split {
            ratio_percent: 40,
            ..
        }
    ));
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus("Resized pane left".to_string())]
    );
}

#[test]
fn dispatch_pane_resize_reports_when_no_matching_split_exists() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "pane.resize.left",
        "Grow the active pane to the left",
        CommandScope::Workspace,
        CommandSpec::resize_pane_left(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "pane.resize.left");

    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "No pane to resize left".to_string()
        )]
    );
}

#[test]
fn dispatch_package_run_creates_job_and_records_buffer() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "package.run",
        "Run a Zacor package command",
        CommandScope::Workspace,
        CommandSpec::package_run(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "package.run echo default value=hi");

    let Some(CommandData::PackageJobStarted { job_id, buffer_id }) = result.data() else {
        panic!("expected package job data, got {:?}", result.data());
    };
    let buffer = workspace
        .buffer(*buffer_id)
        .expect("records buffer should exist");
    let job = workspace.jobs().get(*job_id).expect("job should exist");

    assert!(matches!(buffer.content(), BufferContent::Records(_)));
    assert_eq!(buffer.name(), "*pkg:echo default*");
    assert_eq!(workspace.current_buffer().id(), *buffer_id);
    assert_eq!(job.name(), "package echo default");
    assert_eq!(
        job.kind(),
        &JobKind::PackageInvoke {
            package: "echo".to_string(),
            command: "default".to_string(),
            output_buffer_id: *buffer_id,
        }
    );
    assert_eq!(
        result.effects(),
        &[
            CommandEffect::SetStatus("Running echo default".to_string()),
            CommandEffect::InvokePackage(PackageInvocationRequest {
                job_id: *job_id,
                buffer_id: *buffer_id,
                package: "echo".to_string(),
                command: "default".to_string(),
                args: [("value".to_string(), "hi".to_string())]
                    .into_iter()
                    .collect(),
            }),
        ]
    );
}

#[test]
fn typed_buffer_append_reports_missing_buffer_as_error() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::BufferAppend {
            buffer_id: BufferId::new(999),
            text: "hello".to_string(),
        },
    );

    assert_eq!(result.error(), Some("unknown buffer id: 999"));
}

#[test]
fn command_usage_is_exposed_by_spec() {
    let command = Command::with_spec(
        "eval",
        "Evaluate Lua code",
        CommandScope::Workspace,
        CommandSpec::eval_lua(),
    );

    assert_eq!(command.usage(), Some("Usage: :eval <lua code>"));
}

#[test]
fn registry_entries_expose_command_metadata() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "eval",
        "Evaluate Lua code",
        CommandScope::Workspace,
        CommandSpec::eval_lua(),
    ));

    let entries = registry.entries().collect::<Vec<_>>();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name(), "eval");
    assert_eq!(entries[0].summary(), "Evaluate Lua code");
    assert_eq!(entries[0].scope(), CommandScope::Workspace);
    assert_eq!(entries[0].usage(), Some("Usage: :eval <lua code>"));
}

#[test]
fn typed_buffer_focus_updates_active_buffer_without_effects() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_text_buffer("notes");

    let result =
        registry.dispatch_invocation(&mut workspace, CommandInvocation::BufferFocus { buffer_id });

    assert!(result.effects().is_empty());
    assert_eq!(result.data(), None);
    assert_eq!(result.error(), None);
    assert_eq!(workspace.current_buffer().name(), "notes");
}
