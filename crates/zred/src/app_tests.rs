use crate::app::App;
use crate::kernel::PackageInvocationRequest;
use crate::kernel::{JobKind, JobStatus, MessageLevel, MinibufferMode};
use crate::session::{PackageRunEvent, PackageRunResult, SessionPackageRuntime};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::json;

struct ExpectedRequest {
    package: &'static str,
    command: &'static str,
    arg_key: Option<&'static str>,
    arg_value: Option<&'static str>,
}

struct TestPackageRunner {
    expected_request: Option<ExpectedRequest>,
    events: Vec<PackageRunEvent>,
    result: Result<PackageRunResult, String>,
}

impl SessionPackageRuntime for TestPackageRunner {
    fn invoke_package(
        &mut self,
        request: &PackageInvocationRequest,
        on_event: &mut dyn FnMut(PackageRunEvent),
    ) -> Result<PackageRunResult, String> {
        if let Some(expected) = &self.expected_request {
            assert_eq!(request.package, expected.package);
            assert_eq!(request.command, expected.command);
            match (expected.arg_key, expected.arg_value) {
                (Some(key), Some(value)) => {
                    assert_eq!(request.args.get(key), Some(&value.to_string()));
                }
                (None, None) => {}
                other => panic!("invalid expected request args: {other:?}"),
            }
        }

        for event in self.events.clone() {
            on_event(event);
        }

        self.result.clone()
    }
}

fn run_command(app: &mut App, command: &str) {
    app.runtime.run_command(command);
}

fn press_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

#[test]
fn quit_command_sets_should_quit() {
    let mut app = App::new().unwrap();
    run_command(&mut app, "q");
    assert!(app.state().should_quit());
}

#[test]
fn unknown_command_updates_minibuffer() {
    let mut app = App::new().unwrap();
    run_command(&mut app, "bogus");
    assert_eq!(app.state().minibuffer().input(), "Unknown command: :bogus");
    assert_eq!(app.state().minibuffer().mode(), MinibufferMode::Message);
}

#[test]
fn new_app_has_single_scratch_buffer() {
    let app = App::new().unwrap();
    assert_eq!(app.state().buffer_count(), 1);
    assert_eq!(app.state().current_buffer().name(), "*scratch*");
}

#[test]
fn eval_command_can_mutate_editor_state() {
    let mut app = App::new().unwrap();
    run_command(&mut app, "eval minibuffer.message('from lua')");
    assert_eq!(app.state().minibuffer().input(), "from lua");
}

#[test]
fn buffer_new_command_routes_through_workspace_registry() {
    let mut app = App::new().unwrap();

    run_command(&mut app, "buffer.new notes");

    assert_eq!(app.state().buffer_count(), 2);
    assert_eq!(app.state().current_buffer().name(), "notes");
    assert_eq!(app.state().minibuffer().input(), "Created notes");
}

#[test]
fn colon_key_enters_command_mode_via_keymap() {
    let mut app = App::new().unwrap();

    press_key(&mut app, KeyCode::Char(':'), KeyModifiers::NONE);

    assert_eq!(app.state().minibuffer().mode(), MinibufferMode::Command);
    assert_eq!(app.state().minibuffer().input(), "");
}

#[test]
fn n_key_creates_next_buffer_via_keymap() {
    let mut app = App::new().unwrap();

    press_key(&mut app, KeyCode::Char('n'), KeyModifiers::NONE);

    assert_eq!(app.state().buffer_count(), 2);
    assert_eq!(app.state().current_buffer().name(), "*buffer-2*");
    assert_eq!(app.state().minibuffer().input(), "Created *buffer-2*");
}

#[test]
fn help_command_lists_registered_commands() {
    let mut app = App::new().unwrap();

    run_command(&mut app, "help");

    assert_eq!(
        app.state().minibuffer().input(),
        "Commands: buffer.new, eval, help, package.run, pane.down, pane.left, pane.next, pane.prev, pane.resize.down, pane.resize.left, pane.resize.right, pane.resize.up, pane.right, pane.split.horizontal, pane.split.vertical, pane.up, q, quit"
    );
}

#[test]
fn help_command_for_named_command_uses_registry_metadata() {
    let mut app = App::new().unwrap();

    run_command(&mut app, "help buffer.new");

    assert_eq!(
        app.state().minibuffer().input(),
        "Create a text buffer: Usage: :buffer.new <name>"
    );
}

#[test]
fn lua_buffer_api_routes_create_and_focus_through_command_path() {
    let mut app = App::new().unwrap();

    run_command(
        &mut app,
        "eval local id = buffer.create('lua-notes'); buffer.focus(id)",
    );

    assert_eq!(app.state().buffer_count(), 2);
    assert_eq!(app.state().current_buffer().name(), "lua-notes");
    assert_eq!(app.state().minibuffer().input(), "Created lua-notes");
}

#[test]
fn lua_buffer_api_surfaces_unknown_buffer_errors() {
    let mut app = App::new().unwrap();

    run_command(&mut app, "eval buffer.focus(999)");

    assert!(
        app.state()
            .minibuffer()
            .input()
            .contains("Lua error: unknown buffer id: 999")
    );
}

#[test]
fn lua_command_run_uses_shared_command_surface() {
    let mut app = App::new().unwrap();

    run_command(&mut app, "eval command.run('buffer.new cmd-notes')");

    assert_eq!(app.state().buffer_count(), 2);
    assert_eq!(app.state().current_buffer().name(), "cmd-notes");
    assert_eq!(app.state().minibuffer().input(), "Created cmd-notes");
}

#[test]
fn lua_command_list_exposes_registry_metadata() {
    let mut app = App::new().unwrap();

    run_command(
        &mut app,
        "eval local commands = command.list(); minibuffer.message(commands[1].name .. '|' .. commands[1].summary)",
    );

    assert_eq!(
        app.state().minibuffer().input(),
        "buffer.new|Create a text buffer"
    );
}

#[test]
fn lua_command_get_exposes_single_command_metadata() {
    let mut app = App::new().unwrap();

    run_command(
        &mut app,
        "eval local cmd = command.get('help'); minibuffer.message(cmd.name .. '|' .. cmd.usage)",
    );

    assert_eq!(
        app.state().minibuffer().input(),
        "help|Usage: :help [command]"
    );
}

#[test]
fn lua_command_get_returns_nil_for_unknown_command() {
    let mut app = App::new().unwrap();

    run_command(
        &mut app,
        "eval local cmd = command.get('missing'); minibuffer.message(tostring(cmd == nil))",
    );

    assert_eq!(app.state().minibuffer().input(), "true");
}

#[test]
fn pane_split_vertical_command_creates_a_second_pane() {
    let mut app = App::new().unwrap();

    run_command(&mut app, "pane.split.vertical");

    assert_eq!(app.state().pane_count(), 2);
    assert_eq!(app.state().active_pane_id(), 2);
    assert_eq!(app.state().current_buffer().name(), "*scratch*");
    assert_eq!(app.state().minibuffer().input(), "Split pane vertically");
}

#[test]
fn pane_next_command_cycles_focus_between_split_panes() {
    let mut app = App::new().unwrap();

    run_command(&mut app, "pane.split.vertical");
    run_command(&mut app, "pane.next");

    assert_eq!(app.state().pane_count(), 2);
    assert_eq!(app.state().active_pane_id(), 1);
    assert_eq!(app.state().current_buffer().name(), "*scratch*");
    assert_eq!(app.state().minibuffer().input(), "Focused pane 1");
}

#[test]
fn pane_right_command_follows_vertical_split_geometry() {
    let mut app = App::new().unwrap();

    run_command(&mut app, "pane.split.vertical");
    run_command(&mut app, "pane.next");
    run_command(&mut app, "pane.right");

    assert_eq!(app.state().pane_count(), 2);
    assert_eq!(app.state().active_pane_id(), 2);
    assert_eq!(app.state().current_buffer().name(), "*scratch*");
    assert_eq!(app.state().minibuffer().input(), "Focused pane 2");
}

#[test]
fn ctrl_w_v_splits_the_active_pane() {
    let mut app = App::new().unwrap();

    press_key(&mut app, KeyCode::Char('w'), KeyModifiers::CONTROL);
    press_key(&mut app, KeyCode::Char('v'), KeyModifiers::NONE);

    assert_eq!(app.state().pane_count(), 2);
    assert_eq!(app.state().active_pane_id(), 2);
    assert_eq!(app.state().minibuffer().input(), "Split pane vertically");
}

#[test]
fn ctrl_w_l_focuses_the_pane_to_the_right() {
    let mut app = App::new().unwrap();

    run_command(&mut app, "pane.split.vertical");
    run_command(&mut app, "pane.next");
    press_key(&mut app, KeyCode::Char('w'), KeyModifiers::CONTROL);
    press_key(&mut app, KeyCode::Char('l'), KeyModifiers::NONE);

    assert_eq!(app.state().active_pane_id(), 2);
    assert_eq!(app.state().minibuffer().input(), "Focused pane 2");
}

#[test]
fn pane_resize_left_command_updates_layout_ratio() {
    let mut app = App::new().unwrap();

    run_command(&mut app, "pane.split.vertical");
    run_command(&mut app, "pane.resize.left");

    assert!(matches!(
        app.state().view().pane_tree,
        crate::session::SessionPaneNode::Split {
            ratio_percent: 40,
            ..
        }
    ));
    assert_eq!(app.state().minibuffer().input(), "Resized pane left");
}

#[test]
fn ctrl_w_shift_l_resizes_the_active_pane_right() {
    let mut app = App::new().unwrap();

    run_command(&mut app, "pane.split.vertical");
    run_command(&mut app, "pane.next");
    press_key(&mut app, KeyCode::Char('w'), KeyModifiers::CONTROL);
    press_key(&mut app, KeyCode::Char('L'), KeyModifiers::NONE);

    assert!(matches!(
        app.state().view().pane_tree,
        crate::session::SessionPaneNode::Split {
            ratio_percent: 60,
            ..
        }
    ));
    assert_eq!(app.state().minibuffer().input(), "Resized pane right");
}

#[test]
fn ctrl_w_unknown_key_sets_status_message() {
    let mut app = App::new().unwrap();

    press_key(&mut app, KeyCode::Char('w'), KeyModifiers::CONTROL);
    press_key(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);

    assert_eq!(app.state().pane_count(), 1);
    assert_eq!(app.state().minibuffer().input(), "Unknown pane key");
}

#[test]
fn package_run_routes_output_into_records_buffer_and_job() {
    let mut app = App::new().unwrap();
    app.runtime.set_package_runner(TestPackageRunner {
        expected_request: Some(ExpectedRequest {
            package: "echo",
            command: "default",
            arg_key: Some("value"),
            arg_value: Some("hello"),
        }),
        events: vec![
            PackageRunEvent::Message {
                level: MessageLevel::Info,
                text: "streaming hello".to_string(),
            },
            PackageRunEvent::Record(json!({"value": "hello"})),
        ],
        result: Ok(PackageRunResult { exit_code: 0 }),
    });

    run_command(&mut app, "package.run echo default value=hello");

    let state = app.state();
    let buffer = state.current_buffer();
    let records = match buffer.content() {
        crate::kernel::BufferContent::Records(content) => content.records(),
        other => panic!("expected records buffer, got {other:?}"),
    };
    let job = state
        .workspace()
        .jobs()
        .entries()
        .next()
        .expect("package job should exist");

    assert_eq!(buffer.name(), "*pkg:echo default*");
    assert_eq!(records, &[json!({"value": "hello"})]);
    assert_eq!(job.status(), &JobStatus::Succeeded);
    assert_eq!(
        job.kind(),
        &JobKind::PackageInvoke {
            package: "echo".to_string(),
            command: "default".to_string(),
            output_buffer_id: buffer.id(),
        }
    );
    let messages = state.workspace().messages().entries();
    let message = messages
        .last()
        .expect("package completion message should exist");

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].level(), MessageLevel::Info);
    assert_eq!(messages[0].text(), "streaming hello");
    assert_eq!(
        state.minibuffer().input(),
        "Finished echo default (1 records)"
    );
    assert_eq!(message.level(), MessageLevel::Info);
    assert_eq!(message.text(), "Finished echo default (1 records)");
}

#[test]
fn package_run_streams_warning_messages_into_workspace_log() {
    let mut app = App::new().unwrap();
    app.runtime.set_package_runner(TestPackageRunner {
        expected_request: None,
        events: vec![
            PackageRunEvent::Message {
                level: MessageLevel::Warning,
                text: "partial results".to_string(),
            },
            PackageRunEvent::Record(json!({"value": "partial"})),
        ],
        result: Ok(PackageRunResult { exit_code: 0 }),
    });

    run_command(&mut app, "package.run echo default");

    let state = app.state();
    let messages = state.workspace().messages().entries();

    assert_eq!(messages[0].level(), MessageLevel::Warning);
    assert_eq!(messages[0].text(), "partial results");
    assert_eq!(
        state.minibuffer().input(),
        "Finished echo default (1 records)"
    );
}

#[test]
fn package_run_nonzero_exit_marks_job_failed_and_sets_warning_status() {
    let mut app = App::new().unwrap();
    app.runtime.set_package_runner(TestPackageRunner {
        expected_request: None,
        events: vec![PackageRunEvent::Record(json!({"value": "partial"}))],
        result: Ok(PackageRunResult { exit_code: 7 }),
    });

    run_command(&mut app, "package.run echo default");

    let state = app.state();
    let job = state
        .workspace()
        .jobs()
        .entries()
        .next()
        .expect("package job should exist");
    let message = state
        .workspace()
        .messages()
        .entries()
        .last()
        .expect("package failure message should exist");

    assert_eq!(
        job.status(),
        &JobStatus::Failed("Package echo default failed with status 7".to_string())
    );
    assert_eq!(
        state.minibuffer().input(),
        "Package echo default failed with status 7"
    );
    assert_eq!(message.level(), MessageLevel::Warning);
    assert_eq!(message.text(), "Package echo default failed with status 7");
}

#[test]
fn package_run_runtime_error_marks_job_failed_and_sets_error_status() {
    let mut app = App::new().unwrap();
    app.runtime.set_package_runner(TestPackageRunner {
        expected_request: None,
        events: Vec::new(),
        result: Err("spawn failed".to_string()),
    });

    run_command(&mut app, "package.run echo default");

    let state = app.state();
    let job = state
        .workspace()
        .jobs()
        .entries()
        .next()
        .expect("package job should exist");
    let message = state
        .workspace()
        .messages()
        .entries()
        .last()
        .expect("package error message should exist");

    assert_eq!(
        job.status(),
        &JobStatus::Failed("Package echo default failed: spawn failed".to_string())
    );
    assert_eq!(
        state.minibuffer().input(),
        "Package echo default failed: spawn failed"
    );
    assert_eq!(message.level(), MessageLevel::Error);
    assert_eq!(message.text(), "Package echo default failed: spawn failed");
}
