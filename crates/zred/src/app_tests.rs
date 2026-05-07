use crate::kernel::PackageInvocationRequest;
use crate::kernel::{JobKind, JobStatus, MessageLevel};
use crate::session::{PackageRunEvent, PackageRunResult, SessionPackageRuntime};
use crate::shell::App;
use serde_json::json;

struct AppHarness {
    app: App,
}

impl AppHarness {
    fn new() -> Self {
        Self {
            app: App::new().unwrap(),
        }
    }

    fn state(&self) -> std::cell::Ref<'_, crate::session::Session> {
        self.app.state()
    }

    fn run_command(&mut self, command: &str) {
        self.app.run_command(command);
    }

    fn set_package_runner(&mut self, runner: impl SessionPackageRuntime + 'static) {
        self.app.set_package_runner(runner);
    }
}

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

fn run_command(app: &mut AppHarness, command: &str) {
    app.run_command(command);
}

#[test]
fn package_run_routes_output_into_records_buffer_and_job() {
    let mut app = AppHarness::new();
    app.set_package_runner(TestPackageRunner {
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
fn package_run_refreshes_open_jobs_buffer_live() {
    let mut app = AppHarness::new();
    app.set_package_runner(TestPackageRunner {
        expected_request: None,
        events: vec![PackageRunEvent::Record(json!({"value": "hello"}))],
        result: Ok(PackageRunResult { exit_code: 0 }),
    });

    run_command(&mut app, "job.open");
    run_command(&mut app, "package.run echo default value=hello");
    run_command(&mut app, "job.open");

    let state = app.state();
    let buffer = state.current_buffer();
    let records = match buffer.content() {
        crate::kernel::BufferContent::Records(content) => content.records(),
        other => panic!("expected records buffer, got {other:?}"),
    };

    assert_eq!(buffer.name(), "*jobs*");
    assert_eq!(
        records,
        &[json!({
            "id": 1,
            "name": "package echo default",
            "status": "succeeded",
            "owner_kind": "workspace",
            "owner_id": 1,
            "kind": "package_invoke",
            "package": "echo",
            "command": "default",
            "output_buffer_id": 3,
            "output_buffer_name": "*pkg:echo default*",
            "has_output": true,
            "summary": "package echo default [succeeded] echo default -> buffer 3",
        })]
    );
}

#[test]
fn package_run_streams_warning_messages_into_workspace_log() {
    let mut app = AppHarness::new();
    app.set_package_runner(TestPackageRunner {
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
fn package_run_progress_messages_are_logged_before_completion() {
    let mut app = AppHarness::new();
    app.set_package_runner(TestPackageRunner {
        expected_request: None,
        events: vec![
            PackageRunEvent::Message {
                level: MessageLevel::Info,
                text: "echo default: 25%".to_string(),
            },
            PackageRunEvent::Message {
                level: MessageLevel::Info,
                text: "echo default: 75%".to_string(),
            },
            PackageRunEvent::Record(json!({"value": "done"})),
        ],
        result: Ok(PackageRunResult { exit_code: 0 }),
    });

    run_command(&mut app, "package.run echo default");

    let state = app.state();
    let messages = state.workspace().messages().entries();

    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].level(), MessageLevel::Info);
    assert_eq!(messages[0].text(), "echo default: 25%");
    assert_eq!(messages[1].level(), MessageLevel::Info);
    assert_eq!(messages[1].text(), "echo default: 75%");
    assert_eq!(messages[2].level(), MessageLevel::Info);
    assert_eq!(messages[2].text(), "Finished echo default (1 records)");
    assert_eq!(
        state.minibuffer().input(),
        "Finished echo default (1 records)"
    );
}

#[test]
fn package_run_error_message_event_is_preserved_in_workspace_log() {
    let mut app = AppHarness::new();
    app.set_package_runner(TestPackageRunner {
        expected_request: None,
        events: vec![PackageRunEvent::Message {
            level: MessageLevel::Error,
            text: "daemon refused request".to_string(),
        }],
        result: Err("dispatch aborted".to_string()),
    });

    run_command(&mut app, "package.run echo default");

    let state = app.state();
    let messages = state.workspace().messages().entries();

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].level(), MessageLevel::Error);
    assert_eq!(messages[0].text(), "daemon refused request");
    assert_eq!(messages[1].level(), MessageLevel::Error);
    assert_eq!(
        messages[1].text(),
        "Package echo default failed: dispatch aborted"
    );
    assert_eq!(
        state.minibuffer().input(),
        "Package echo default failed: dispatch aborted"
    );
}

#[test]
fn package_run_nonzero_exit_marks_job_failed_and_sets_warning_status() {
    let mut app = AppHarness::new();
    app.set_package_runner(TestPackageRunner {
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
    let mut app = AppHarness::new();
    app.set_package_runner(TestPackageRunner {
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
