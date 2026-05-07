use crate::runtime::AppRuntime;
#[cfg(test)]
use crate::session::SessionPackageRuntime;
use crate::kernel::WorkspaceId;
use crate::session::{
    AppInputEvent, Session, SessionFrontendEffect, SessionInputController, SessionView,
};
use anyhow::Result;
#[cfg(test)]
use std::cell::{Ref, RefMut};

pub trait AppShell {
    fn should_quit(&self) -> bool;
    fn view(&self) -> SessionView;
    fn handle_input(&mut self, event: AppInputEvent);
    fn drain_frontend_effects(&mut self) -> Vec<SessionFrontendEffect>;
    fn set_status(&mut self, status: &str);
}

pub struct App {
    input: SessionInputController,
    runtime: AppRuntime,
}

impl App {
    pub fn new() -> Result<Self> {
        Self::with_session(Session::shared())
    }

    pub fn with_workspace_id(workspace_id: WorkspaceId) -> Result<Self> {
        Self::with_session(Session::shared_with_workspace_id(workspace_id))
    }

    fn with_session(state: crate::session::SharedSession) -> Result<Self> {
        Ok(Self {
            input: SessionInputController::new(state.clone()),
            runtime: AppRuntime::new(state)?,
        })
    }

    pub fn should_quit(&self) -> bool {
        self.runtime.state().should_quit()
    }

    pub fn workspace_id(&self) -> crate::kernel::WorkspaceId {
        self.runtime.state().workspace().id()
    }

    pub fn set_workspace_id(&mut self, workspace_id: WorkspaceId) {
        self.runtime.set_workspace_id(workspace_id);
    }

    pub fn drain_frontend_effects(&mut self) -> Vec<SessionFrontendEffect> {
        self.runtime.drain_frontend_effects()
    }

    pub fn set_status(&mut self, status: &str) {
        self.runtime.set_status(status);
    }

    #[cfg(test)]
    pub fn state(&self) -> Ref<'_, Session> {
        self.runtime.state()
    }

    #[cfg(test)]
    pub fn state_mut(&self) -> RefMut<'_, Session> {
        self.runtime.state_mut()
    }

    pub fn view(&self) -> SessionView {
        self.runtime.state().view()
    }

    pub fn focus_pane(&mut self, pane_id: crate::kernel::PaneId) -> bool {
        self.runtime.focus_pane(pane_id)
    }

    pub fn adjust_active_pane_viewport(&mut self, delta_x: isize, delta_y: isize) -> bool {
        self.runtime.adjust_active_pane_viewport(delta_x, delta_y)
    }

    pub fn select_record_row_in_active_pane(&mut self, row: usize) -> bool {
        self.runtime.select_record_row_in_active_pane(row)
    }

    pub fn select_tree_node_in_active_pane(&mut self, node_id: &str) -> bool {
        self.runtime.select_tree_node_in_active_pane(node_id)
    }

    pub fn open_active_structured_selection(&mut self) {
        self.runtime.open_active_structured_selection();
    }

    pub fn handle_input(&mut self, event: AppInputEvent) {
        let input = &mut self.input;
        let runtime = &mut self.runtime;
        let (lua, package_runtime) = runtime.runtimes();
        input.handle_input(event, lua, package_runtime);
    }

    pub(crate) fn run_command(&mut self, command: &str) {
        self.runtime.run_command(command);
    }

    #[cfg(test)]
    pub(crate) fn set_package_runner(&mut self, runner: impl SessionPackageRuntime + 'static) {
        self.runtime.set_package_runner(runner);
    }
}

impl AppShell for App {
    fn should_quit(&self) -> bool {
        Self::should_quit(self)
    }

    fn view(&self) -> SessionView {
        Self::view(self)
    }

    fn handle_input(&mut self, event: AppInputEvent) {
        Self::handle_input(self, event)
    }

    fn drain_frontend_effects(&mut self) -> Vec<SessionFrontendEffect> {
        Self::drain_frontend_effects(self)
    }

    fn set_status(&mut self, status: &str) {
        Self::set_status(self, status)
    }
}

#[cfg(test)]
mod tests {
    use super::App;
    use super::AppShell;
    use crate::kernel::{BufferKind, KeyChord, KeyCodeRepr, KeyModifiersRepr, MinibufferMode};
    use crate::session::{
        AppInputEvent, PackageRunEvent, PackageRunResult, SessionPackageRuntime,
        SessionPaneContentView, SessionPaneNode,
    };
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestPackageRunner {
        events: Vec<PackageRunEvent>,
        result: Result<PackageRunResult, String>,
    }

    impl SessionPackageRuntime for TestPackageRunner {
        fn invoke_package(
            &mut self,
            _request: &crate::kernel::PackageInvocationRequest,
            on_event: &mut dyn FnMut(PackageRunEvent),
        ) -> Result<PackageRunResult, String> {
            for event in self.events.clone() {
                on_event(event);
            }
            self.result.clone()
        }
    }

    fn send_command(shell: &mut dyn AppShell, command: &str) {
        shell.handle_input(AppInputEvent::new(
            Some(KeyChord::new(
                KeyCodeRepr::Char(':'),
                KeyModifiersRepr::NONE,
            )),
            Some(':'),
        ));
        for ch in command.chars() {
            shell.handle_input(AppInputEvent::new(
                Some(KeyChord::new(KeyCodeRepr::Char(ch), KeyModifiersRepr::NONE)),
                Some(ch),
            ));
        }
        shell.handle_input(AppInputEvent::new(
            Some(KeyChord::new(KeyCodeRepr::Enter, KeyModifiersRepr::NONE)),
            None,
        ));
    }

    fn press_key(shell: &mut dyn AppShell, code: KeyCodeRepr, text_input: Option<char>) {
        shell.handle_input(AppInputEvent::new(
            Some(KeyChord::new(code, KeyModifiersRepr::NONE)),
            text_input,
        ));
    }

    fn press_ctrl_key(shell: &mut dyn AppShell, code: KeyCodeRepr) {
        shell.handle_input(AppInputEvent::new(
            Some(KeyChord::new(code, KeyModifiersRepr::CONTROL)),
            None,
        ));
    }

    fn press_key_with_modifiers(
        shell: &mut dyn AppShell,
        code: KeyCodeRepr,
        modifiers: KeyModifiersRepr,
        text_input: Option<char>,
    ) {
        shell.handle_input(AppInputEvent::new(
            Some(KeyChord::new(code, modifiers)),
            text_input,
        ));
    }

    #[test]
    fn shell_contract_exposes_view_and_accepts_input() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        let initial = shell.view();
        let SessionPaneNode::Leaf(pane) = &initial.pane_tree else {
            panic!("expected leaf pane view");
        };
        assert_eq!(pane.buffer_id, 1);
        assert_eq!(pane.buffer_kind, BufferKind::Text);
        assert_eq!(initial.minibuffer_text, "Ready");
        assert!(!shell.should_quit());

        shell.handle_input(AppInputEvent::new(
            Some(KeyChord::new(
                KeyCodeRepr::Char(':'),
                KeyModifiersRepr::NONE,
            )),
            Some(':'),
        ));

        let command_view = shell.view();
        assert_eq!(command_view.minibuffer_mode, MinibufferMode::Command);
        assert_eq!(command_view.minibuffer_text, "");

        shell.handle_input(AppInputEvent::new(
            Some(KeyChord::new(
                KeyCodeRepr::Char('q'),
                KeyModifiersRepr::NONE,
            )),
            Some('q'),
        ));
        shell.handle_input(AppInputEvent::new(
            Some(KeyChord::new(KeyCodeRepr::Enter, KeyModifiersRepr::NONE)),
            None,
        ));

        assert!(shell.should_quit());
    }

    #[test]
    fn shell_contract_drives_split_pane_commands() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "pane.split.vertical");

        let view = shell.view();
        assert!(matches!(
            view.pane_tree,
            SessionPaneNode::Split {
                ratio_percent: 50,
                ..
            }
        ));
        assert_eq!(view.minibuffer_text, "Split pane vertically");
        assert!(!shell.should_quit());
    }

    #[test]
    fn shell_contract_surfaces_package_run_output_in_view() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: vec![PackageRunEvent::Record(json!({"value": "hello"}))],
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default value=hello");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };
        assert_eq!(pane.buffer_name, "*pkg:echo default*");
        assert_eq!(pane.buffer_kind, BufferKind::Records);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Records(vec![json!({"value": "hello"})])
        );
        assert_eq!(view.minibuffer_text, "Finished echo default (1 records)");
        assert!(!shell.should_quit());
    }

    #[test]
    fn shell_contract_routes_keymap_input_without_tui_helpers() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        press_key(shell, KeyCodeRepr::Char('n'), Some('n'));

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*buffer-2*");
        assert_eq!(pane.buffer_kind, BufferKind::Text);
        assert_eq!(view.minibuffer_text, "Created *buffer-2*");
    }

    #[test]
    fn shell_contract_surfaces_unknown_pane_key_status() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        press_ctrl_key(shell, KeyCodeRepr::Char('w'));
        press_key(shell, KeyCodeRepr::Char('x'), Some('x'));

        let view = shell.view();

        assert_eq!(view.minibuffer_mode, MinibufferMode::Message);
        assert_eq!(view.minibuffer_text, "Unknown pane key");
    }

    #[test]
    fn shell_contract_surfaces_unknown_command_status() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "bogus");

        let view = shell.view();

        assert_eq!(view.minibuffer_mode, MinibufferMode::Message);
        assert_eq!(view.minibuffer_text, "Unknown command: :bogus");
    }

    #[test]
    fn shell_contract_surfaces_help_output() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "help");

        let view = shell.view();

        assert_eq!(
            view.minibuffer_text,
            "Commands: browser.open, browser.title.set, browser.url.set, buffer.describe, buffer.new, buffer.record.current, buffer.record.next, buffer.record.open, buffer.record.prev, buffer.structured.current, buffer.structured.next, buffer.structured.open, buffer.structured.prev, buffer.tree.current, buffer.tree.next, buffer.tree.open, buffer.tree.prev, canvas.name.set, canvas.open, eval, help, job.cancel, job.describe, job.focus-output, job.list, job.next, job.open, job.prev, media.open, media.source.set, package.run, pane.down, pane.left, pane.next, pane.prev, pane.resize.down, pane.resize.left, pane.resize.right, pane.resize.up, pane.right, pane.split.horizontal, pane.split.vertical, pane.up, q, quit, terminal.append, terminal.open, window.new, workspace.load, workspace.save"
        );
    }

    #[test]
    fn shell_contract_routes_job_list_command() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: vec![PackageRunEvent::Record(json!({"value": "hello"}))],
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default value=hello");
        send_command(shell, "job.list");

        let view = shell.view();

        assert_eq!(
            view.minibuffer_text,
            "Jobs: 1:package echo default [succeeded]"
        );
    }

    #[test]
    fn shell_contract_routes_generic_records_buffer_keys() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: vec![
                PackageRunEvent::Record(json!({"value": "one"})),
                PackageRunEvent::Record(json!({"value": "two"})),
            ],
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default value=hello");
        press_key(shell, KeyCodeRepr::Char('j'), Some('j'));

        let moved_view = shell.view();
        assert_eq!(moved_view.minibuffer_text, "Selected record row 2");

        send_command(shell, "buffer.structured.current");

        let current_view = shell.view();
        assert_eq!(current_view.minibuffer_text, "Record: {\"value\":\"two\"}");
    }

    #[test]
    fn shell_contract_routes_generic_record_open_key() {
        let mut app = App::new().expect("app should initialize");
        let target = app.state_mut().workspace_mut().create_text_buffer("target");
        let results = app
            .state_mut()
            .workspace_mut()
            .create_records_buffer("results");
        assert!(app.state_mut().workspace_mut().push_record_to_buffer(
            results,
            json!({"buffer_id": target.raw(), "label": "linked"})
        ));
        assert!(app.state_mut().workspace_mut().focus_buffer(results));
        let shell: &mut dyn AppShell = &mut app;

        press_key(shell, KeyCodeRepr::Enter, None);

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "target");
        assert_eq!(
            view.minibuffer_text,
            format!("Focused linked buffer {target}")
        );
    }

    #[test]
    fn shell_contract_routes_job_describe_command() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: vec![PackageRunEvent::Record(json!({"value": "hello"}))],
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default value=hello");
        send_command(shell, "job.describe 1");

        let view = shell.view();

        assert_eq!(
            view.minibuffer_text,
            "Job 1: package echo default [succeeded] echo default -> buffer 2"
        );
    }

    #[test]
    fn shell_contract_routes_job_open_command() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: vec![PackageRunEvent::Record(json!({"value": "hello"}))],
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default value=hello");
        send_command(shell, "job.open");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*jobs*");
        assert_eq!(pane.buffer_kind, BufferKind::Records);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Records(vec![json!({
                "id": 1,
                "name": "package echo default",
                "status": "succeeded",
                "owner_kind": "workspace",
                "owner_id": 1,
                "kind": "package_invoke",
                "package": "echo",
                "command": "default",
                "output_buffer_id": 2,
                "output_buffer_name": "*pkg:echo default*",
                "has_output": true,
                "summary": "package echo default [succeeded] echo default -> buffer 2",
            })])
        );
        assert_eq!(view.minibuffer_text, "Opened jobs buffer (1)");
    }

    #[test]
    fn shell_contract_routes_job_open_command_reuses_existing_buffer() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: vec![PackageRunEvent::Record(json!({"value": "hello"}))],
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "job.open");
        let first_view = shell.view();
        let SessionPaneNode::Leaf(first_pane) = first_view.pane_tree else {
            panic!("expected leaf pane view");
        };
        let first_jobs_buffer_id = first_pane.buffer_id;

        send_command(shell, "package.run echo default value=hello");
        send_command(shell, "job.open");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_id, first_jobs_buffer_id);
        assert_eq!(pane.buffer_name, "*jobs*");
        assert_eq!(pane.buffer_kind, BufferKind::Records);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Records(vec![json!({
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
            })])
        );
        assert_eq!(view.minibuffer_text, "Refreshed jobs buffer (1)");
    }

    #[test]
    fn shell_contract_routes_job_focus_output_command() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: vec![PackageRunEvent::Record(json!({"value": "hello"}))],
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default value=hello");
        send_command(shell, "job.open");
        send_command(shell, "job.focus-output 1");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*pkg:echo default*");
        assert_eq!(pane.buffer_kind, BufferKind::Records);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Records(vec![json!({"value": "hello"})])
        );
        assert_eq!(
            view.minibuffer_text,
            "Focused output buffer 2 for job 1 (echo default)"
        );
    }

    #[test]
    fn shell_contract_routes_contextual_job_describe_command() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: vec![PackageRunEvent::Record(json!({"value": "hello"}))],
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default value=hello");
        send_command(
            shell,
            "eval local id = job.open(); buffer.focus(id); buffer.select_record(0)",
        );
        send_command(shell, "job.describe");

        let view = shell.view();

        assert_eq!(
            view.minibuffer_text,
            "Job 1: package echo default [succeeded] echo default -> buffer 2"
        );
    }

    #[test]
    fn shell_contract_routes_contextual_job_focus_output_command() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: vec![PackageRunEvent::Record(json!({"value": "hello"}))],
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default value=hello");
        send_command(
            shell,
            "eval local id = job.open(); buffer.focus(id); buffer.select_record(0)",
        );
        send_command(shell, "job.focus-output");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*pkg:echo default*");
        assert_eq!(
            view.minibuffer_text,
            "Focused output buffer 2 for job 1 (echo default)"
        );
    }

    #[test]
    fn shell_contract_routes_contextual_job_cancel_command() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: Vec::new(),
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default");
        send_command(
            shell,
            "eval local id = job.open(); buffer.focus(id); buffer.select_record(0)",
        );
        send_command(shell, "job.cancel");

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "Cancelled job 1");
        assert_eq!(format!("{:?}", view.jobs[0].status), "Cancelled");
    }

    #[test]
    fn shell_contract_routes_job_next_and_prev_commands() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: Vec::new(),
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default");
        send_command(shell, "eval local id = job.open(); buffer.focus(id)");
        send_command(shell, "job.next");

        let next_view = shell.view();
        assert_eq!(next_view.minibuffer_text, "Selected job row 1");

        send_command(shell, "job.prev");

        let prev_view = shell.view();
        assert_eq!(prev_view.minibuffer_text, "Selected job row 1");
    }

    #[test]
    fn shell_contract_routes_direct_jobs_buffer_keys() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: vec![PackageRunEvent::Record(json!({"value": "hello"}))],
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default value=hello");
        send_command(shell, "job.open");
        press_key(shell, KeyCodeRepr::Char('j'), Some('j'));

        let selected_view = shell.view();
        assert_eq!(selected_view.minibuffer_text, "Selected job row 1");
        assert_eq!(
            selected_view.selected_job.as_ref().map(|job| job.id),
            Some(1)
        );

        press_key(shell, KeyCodeRepr::Char('d'), Some('d'));
        let described_view = shell.view();
        assert_eq!(
            described_view.minibuffer_text,
            "Job 1: package echo default [succeeded] echo default -> buffer 2"
        );

        press_key(shell, KeyCodeRepr::Enter, None);
        let focused_view = shell.view();
        let SessionPaneNode::Leaf(pane) = focused_view.pane_tree else {
            panic!("expected leaf pane view");
        };
        assert_eq!(pane.buffer_name, "*pkg:echo default*");
        assert_eq!(
            focused_view.minibuffer_text,
            "Focused output buffer 2 for job 1 (echo default)"
        );
    }

    #[test]
    fn shell_contract_routes_direct_jobs_buffer_cancel_key() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: Vec::new(),
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default");
        send_command(shell, "job.open");
        press_key(shell, KeyCodeRepr::Char('j'), Some('j'));
        press_key(shell, KeyCodeRepr::Char('c'), Some('c'));

        let view = shell.view();
        assert_eq!(view.minibuffer_text, "Cancelled job 1");
        assert_eq!(
            view.selected_job.as_ref().map(|job| &job.status),
            Some(&crate::session::SessionJobStatusView::Cancelled)
        );
    }

    #[test]
    fn shell_contract_routes_job_cancel_command() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: Vec::new(),
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default");
        send_command(shell, "job.cancel 1");

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "Cancelled job 1");
        assert_eq!(format!("{:?}", view.jobs[0].status), "Cancelled");
    }

    #[test]
    fn shell_contract_routes_ctrl_w_split_without_tui_helpers() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        press_ctrl_key(shell, KeyCodeRepr::Char('w'));
        press_key(shell, KeyCodeRepr::Char('v'), Some('v'));

        let view = shell.view();

        assert!(matches!(
            view.pane_tree,
            SessionPaneNode::Split {
                ratio_percent: 50,
                ..
            }
        ));
        assert_eq!(view.minibuffer_text, "Split pane vertically");
    }

    #[test]
    fn shell_contract_routes_ctrl_w_resize_without_tui_helpers() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "pane.split.vertical");
        send_command(shell, "pane.next");
        press_ctrl_key(shell, KeyCodeRepr::Char('w'));
        press_key_with_modifiers(
            shell,
            KeyCodeRepr::Char('L'),
            KeyModifiersRepr::NONE,
            Some('L'),
        );

        let view = shell.view();

        assert!(matches!(
            view.pane_tree,
            SessionPaneNode::Split {
                ratio_percent: 60,
                ..
            }
        ));
        assert_eq!(view.minibuffer_text, "Resized pane right");
    }

    #[test]
    fn shell_contract_surfaces_named_help_output() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "help buffer.new");

        let view = shell.view();

        assert_eq!(
            view.minibuffer_text,
            "Create a text buffer: Usage: :buffer.new <name>"
        );
    }

    #[test]
    fn shell_contract_routes_buffer_new_command() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "buffer.new notes");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "notes");
        assert_eq!(pane.buffer_kind, BufferKind::Text);
        assert_eq!(view.minibuffer_text, "Created notes");
    }

    #[test]
    fn shell_contract_routes_terminal_open_command() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "terminal.open shell");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*terminal:shell*");
        assert_eq!(pane.buffer_kind, BufferKind::Terminal);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Terminal {
                transcript: Vec::new(),
            }
        );
        assert_eq!(view.minibuffer_text, "Opened terminal shell");
    }

    #[test]
    fn shell_contract_routes_terminal_append_command() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "terminal.open shell");
        send_command(shell, "terminal.append hello");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*terminal:shell*");
        assert_eq!(pane.buffer_kind, BufferKind::Terminal);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Terminal {
                transcript: vec!["hello".to_string()],
            }
        );
        assert_eq!(view.minibuffer_text, "Appended terminal output for 2");
    }

    #[test]
    fn shell_contract_routes_buffer_describe_command_for_terminal_buffer() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "terminal.open shell");
        send_command(shell, "terminal.append hello");
        send_command(shell, "buffer.describe");

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "Buffer 2: terminal (1 lines)");
    }

    #[test]
    fn shell_contract_routes_buffer_describe_command_for_text_buffer() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "buffer.describe");

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "Buffer 1: text (1 lines)");
    }

    #[test]
    fn shell_contract_routes_browser_open_command() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "browser.open https://example.com");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*browser:https://example.com*");
        assert_eq!(pane.buffer_kind, BufferKind::Browser);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Browser {
                url: Some("https://example.com".to_string()),
                title: None,
            }
        );
        assert_eq!(view.minibuffer_text, "Opened https://example.com");
    }

    #[test]
    fn shell_contract_routes_buffer_describe_command_for_browser_buffer() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "browser.open https://example.com");
        send_command(shell, "browser.title.set Example Domain");
        send_command(shell, "buffer.describe");

        let view = shell.view();

        assert_eq!(
            view.minibuffer_text,
            "Buffer 2: browser https://example.com (Example Domain)"
        );
    }

    #[test]
    fn shell_contract_routes_browser_title_set_command() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "browser.open https://example.com");
        send_command(shell, "browser.title.set Example Domain");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*browser:https://example.com*");
        assert_eq!(pane.buffer_kind, BufferKind::Browser);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Browser {
                url: Some("https://example.com".to_string()),
                title: Some("Example Domain".to_string()),
            }
        );
        assert_eq!(view.minibuffer_text, "Set browser title for 2");
    }

    #[test]
    fn shell_contract_routes_browser_url_set_command() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "browser.open https://before.example");
        send_command(shell, "browser.url.set https://after.example");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*browser:https://before.example*");
        assert_eq!(pane.buffer_kind, BufferKind::Browser);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Browser {
                url: Some("https://after.example".to_string()),
                title: None,
            }
        );
        assert_eq!(view.minibuffer_text, "Set browser url for 2");
    }

    #[test]
    fn shell_contract_routes_media_open_command() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "media.open ./clip.mp4");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*media:./clip.mp4*");
        assert_eq!(pane.buffer_kind, BufferKind::Media);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Media {
                source: Some("./clip.mp4".to_string()),
            }
        );
        assert_eq!(view.minibuffer_text, "Opened ./clip.mp4");
    }

    #[test]
    fn shell_contract_routes_buffer_describe_command_for_media_buffer() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "media.open ./clip.mp4");
        send_command(shell, "buffer.describe");

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "Buffer 2: media ./clip.mp4");
    }

    #[test]
    fn shell_contract_routes_media_source_set_command() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "media.open ./before.mp4");
        send_command(shell, "media.source.set ./after.mp4");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*media:./before.mp4*");
        assert_eq!(pane.buffer_kind, BufferKind::Media);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Media {
                source: Some("./after.mp4".to_string()),
            }
        );
        assert_eq!(view.minibuffer_text, "Set media source for 2");
    }

    #[test]
    fn shell_contract_routes_canvas_open_command() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "canvas.open playground");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*canvas:playground*");
        assert_eq!(pane.buffer_kind, BufferKind::Canvas);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Canvas {
                name: Some("playground".to_string()),
            }
        );
        assert_eq!(view.minibuffer_text, "Opened playground");
    }

    #[test]
    fn shell_contract_routes_buffer_describe_command_for_canvas_buffer() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "canvas.open playground");
        send_command(shell, "buffer.describe");

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "Buffer 2: canvas playground");
    }

    #[test]
    fn shell_contract_routes_canvas_name_set_command() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "canvas.open before");
        send_command(shell, "canvas.name.set after");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*canvas:before*");
        assert_eq!(pane.buffer_kind, BufferKind::Canvas);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Canvas {
                name: Some("after".to_string()),
            }
        );
        assert_eq!(view.minibuffer_text, "Set canvas name for 2");
    }

    #[test]
    fn shell_contract_routes_pane_next_command() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "pane.split.vertical");
        send_command(shell, "pane.next");

        let view = shell.view();
        let SessionPaneNode::Split { first, second, .. } = view.pane_tree else {
            panic!("expected split pane view");
        };
        let SessionPaneNode::Leaf(first_pane) = *first else {
            panic!("expected first leaf pane");
        };
        let SessionPaneNode::Leaf(second_pane) = *second else {
            panic!("expected second leaf pane");
        };

        assert!(first_pane.active);
        assert!(!second_pane.active);
        assert_eq!(view.minibuffer_text, "Focused pane 1");
    }

    #[test]
    fn shell_contract_routes_pane_right_command() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "pane.split.vertical");
        send_command(shell, "pane.next");
        send_command(shell, "pane.right");

        let view = shell.view();
        let SessionPaneNode::Split { first, second, .. } = view.pane_tree else {
            panic!("expected split pane view");
        };
        let SessionPaneNode::Leaf(first_pane) = *first else {
            panic!("expected first leaf pane");
        };
        let SessionPaneNode::Leaf(second_pane) = *second else {
            panic!("expected second leaf pane");
        };

        assert!(!first_pane.active);
        assert!(second_pane.active);
        assert_eq!(view.minibuffer_text, "Focused pane 2");
    }

    #[test]
    fn shell_contract_routes_ctrl_w_focus_without_tui_helpers() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "pane.split.vertical");
        send_command(shell, "pane.next");
        press_ctrl_key(shell, KeyCodeRepr::Char('w'));
        press_key(shell, KeyCodeRepr::Char('l'), Some('l'));

        let view = shell.view();
        let SessionPaneNode::Split { first, second, .. } = view.pane_tree else {
            panic!("expected split pane view");
        };
        let SessionPaneNode::Leaf(first_pane) = *first else {
            panic!("expected first leaf pane");
        };
        let SessionPaneNode::Leaf(second_pane) = *second else {
            panic!("expected second leaf pane");
        };

        assert!(!first_pane.active);
        assert!(second_pane.active);
        assert_eq!(view.minibuffer_text, "Focused pane 2");
    }

    #[test]
    fn shell_contract_starts_with_scratch_buffer() {
        let app = App::new().expect("app should initialize");
        let view = app.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*scratch*");
        assert_eq!(pane.buffer_kind, BufferKind::Text);
    }

    #[test]
    fn shell_contract_starts_with_default_workspace_id() {
        let app = App::new().expect("app should initialize");

        assert_eq!(app.workspace_id(), crate::kernel::WorkspaceId::new(1));
    }

    #[test]
    fn shell_contract_can_retag_workspace_id() {
        let mut app = App::new().expect("app should initialize");

        app.set_workspace_id(crate::kernel::WorkspaceId::new(17));

        assert_eq!(app.workspace_id(), crate::kernel::WorkspaceId::new(17));
    }

    #[test]
    fn shell_contract_routes_window_new_command_into_frontend_effects() {
        let mut app = App::new().expect("app should initialize");

        app.run_command("window.new");

        assert_eq!(
            app.drain_frontend_effects(),
            vec![crate::session::SessionFrontendEffect::NewWindow]
        );
        assert_eq!(app.view().minibuffer_text, "Opened a new window");
    }

    #[test]
    fn shell_contract_routes_lua_window_new_into_frontend_effects() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "eval app.new_window()");

        assert_eq!(
            app.drain_frontend_effects(),
            vec![crate::session::SessionFrontendEffect::NewWindow]
        );
        assert_eq!(app.view().minibuffer_text, "Opened a new window");
    }

    #[test]
    fn shell_contract_routes_eval_status_updates() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "eval minibuffer.message('from lua')");

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "from lua");
    }

    #[test]
    fn shell_contract_routes_lua_buffer_create_and_focus() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval local id = buffer.create('lua-notes'); buffer.focus(id)",
        );

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "lua-notes");
        assert_eq!(pane.buffer_kind, BufferKind::Text);
        assert_eq!(view.minibuffer_text, "Created lua-notes");
    }

    #[test]
    fn shell_contract_routes_lua_terminal_buffer_open() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "eval buffer.open_terminal('shell')");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*terminal:shell*");
        assert_eq!(pane.buffer_kind, BufferKind::Terminal);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Terminal {
                transcript: Vec::new(),
            }
        );
        assert_eq!(view.minibuffer_text, "Opened terminal shell");
    }

    #[test]
    fn shell_contract_routes_lua_terminal_append() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval local id = buffer.open_terminal('shell'); buffer.append_terminal(id, 'hello')",
        );

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*terminal:shell*");
        assert_eq!(pane.buffer_kind, BufferKind::Terminal);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Terminal {
                transcript: vec!["hello".to_string()],
            }
        );
        assert_eq!(view.minibuffer_text, "Appended terminal output for 2");
    }

    #[test]
    fn shell_contract_routes_lua_current_terminal_append() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval buffer.open_terminal('shell'); buffer.append_current_terminal('hello')",
        );

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*terminal:shell*");
        assert_eq!(pane.buffer_kind, BufferKind::Terminal);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Terminal {
                transcript: vec!["hello".to_string()],
            }
        );
        assert_eq!(view.minibuffer_text, "Appended terminal output for 2");
    }

    #[test]
    fn shell_contract_routes_lua_workspace_save_and_load() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("zred-shell-workspace-{unique}.json"));
        let path_literal = path.to_string_lossy().replace('\\', "\\\\");

        send_command(
            shell,
            &format!(
                "eval buffer.open_terminal('shell'); buffer.append_current_terminal('hello'); minibuffer.message('saved status'); buffer.save_workspace('{path_literal}')"
            ),
        );
        send_command(shell, "buffer.new throwaway");
        send_command(
            shell,
            &format!("eval buffer.load_workspace('{path_literal}')"),
        );

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };
        std::fs::remove_file(&path).expect("snapshot file should be removed");

        assert_eq!(pane.buffer_name, "*terminal:shell*");
        assert_eq!(pane.buffer_kind, BufferKind::Terminal);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Terminal {
                transcript: vec!["hello".to_string()],
            }
        );
        assert_eq!(view.minibuffer_text, "saved status");
    }

    #[test]
    fn shell_contract_routes_workspace_save_and_load_commands() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("zred-shell-command-workspace-{unique}.json"));
        let path_arg = path.to_string_lossy().to_string();

        send_command(shell, "terminal.open shell");
        send_command(shell, "terminal.append hello");
        send_command(shell, &format!("workspace.save {path_arg}"));
        assert!(path.exists());
        send_command(shell, "buffer.new throwaway");
        send_command(shell, &format!("workspace.load {path_arg}"));

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };
        std::fs::remove_file(&path).expect("snapshot file should be removed");

        assert_eq!(pane.buffer_name, "*terminal:shell*");
        assert_eq!(pane.buffer_kind, BufferKind::Terminal);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Terminal {
                transcript: vec!["hello".to_string()],
            }
        );
        assert_eq!(
            view.minibuffer_text,
            format!("Loaded workspace from {}", path.to_string_lossy())
        );
    }

    #[test]
    fn shell_contract_restores_package_job_state_after_workspace_load() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: vec![PackageRunEvent::Record(json!({"value": "hello"}))],
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("zred-shell-job-workspace-{unique}.json"));
        let path_arg = path.to_string_lossy().to_string();

        app.run_command("package.run echo default value=hello");
        app.run_command(&format!("workspace.save {path_arg}"));
        assert!(path.exists());
        app.run_command("buffer.new throwaway");
        app.run_command(&format!("workspace.load {path_arg}"));

        let state = app.state();
        let view = state.view();
        let job = state
            .workspace()
            .jobs()
            .entries()
            .next()
            .expect("package job should restore");
        std::fs::remove_file(&path).expect("snapshot file should be removed");

        assert_eq!(job.status(), &crate::kernel::JobStatus::Succeeded);
        assert_eq!(
            job.kind(),
            &crate::kernel::JobKind::PackageInvoke {
                package: "echo".to_string(),
                command: "default".to_string(),
                output_buffer_id: crate::kernel::BufferId::new(2),
            }
        );
        assert_eq!(view.jobs.len(), 1);
        assert_eq!(view.jobs[0].id, job.id().raw());
        assert_eq!(view.jobs[0].name, "package echo default");
        assert_eq!(format!("{:?}", view.jobs[0].status), "Succeeded");
        let kind = format!("{:?}", view.jobs[0].kind);
        assert!(kind.contains("PackageInvoke"));
        assert!(kind.contains("echo"));
        assert!(kind.contains("default"));
        assert!(kind.contains("output_buffer_id: 2"));
        assert_eq!(
            state.minibuffer().input(),
            format!("Loaded workspace from {path_arg}")
        );
    }

    #[test]
    fn shell_contract_surfaces_lua_job_metadata() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: vec![PackageRunEvent::Record(json!({"value": "hello"}))],
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default value=hello");
        send_command(
            shell,
            "eval local j = job.list()[1]; minibuffer.message(j.status .. '|' .. j.kind .. '|' .. j.package .. '|' .. j.command)",
        );

        let view = shell.view();

        assert_eq!(
            view.minibuffer_text,
            "succeeded|package_invoke|echo|default"
        );
        assert_eq!(view.jobs.len(), 1);
    }

    #[test]
    fn shell_contract_routes_lua_job_open() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: vec![PackageRunEvent::Record(json!({"value": "hello"}))],
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default value=hello");
        send_command(shell, "eval local id = job.open(); buffer.focus(id)");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*jobs*");
        assert_eq!(pane.buffer_kind, BufferKind::Records);
        assert_eq!(view.minibuffer_text, "Opened jobs buffer (1)");
    }

    #[test]
    fn shell_contract_routes_lua_job_focus_output() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: vec![PackageRunEvent::Record(json!({"value": "hello"}))],
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default value=hello");
        send_command(shell, "eval job.focus_output(1)");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*pkg:echo default*");
        assert_eq!(pane.buffer_kind, BufferKind::Records);
        assert_eq!(
            view.minibuffer_text,
            "Focused output buffer 2 for job 1 (echo default)"
        );
    }

    #[test]
    fn shell_contract_surfaces_lua_current_job_metadata() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: vec![PackageRunEvent::Record(json!({"value": "hello"}))],
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default value=hello");
        send_command(
            shell,
            "eval local id = job.open(); buffer.focus(id); buffer.select_record(0); local j = job.current(); minibuffer.message(j.name .. '|' .. j.status)",
        );

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "package echo default|succeeded");
    }

    #[test]
    fn shell_contract_routes_lua_job_next_and_prev() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: Vec::new(),
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default");
        send_command(
            shell,
            "eval local id = job.open(); buffer.focus(id); job.next(); local j = job.current(); minibuffer.message(j.name)",
        );

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "package echo default");
    }

    #[test]
    fn shell_contract_routes_lua_generic_record_navigation() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: vec![
                PackageRunEvent::Record(json!({"value": "one"})),
                PackageRunEvent::Record(json!({"value": "two"})),
            ],
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default value=hello");
        send_command(
            shell,
            "eval buffer.next_structured_item(); local item = buffer.current_structured_item(); minibuffer.message(tostring(item.row) .. '|' .. item.value.value)",
        );

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "2|two");
    }

    #[test]
    fn shell_contract_routes_lua_current_structured_record() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: vec![PackageRunEvent::Record(json!({"value": "one"}))],
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default value=hello");
        send_command(
            shell,
            "eval local item = buffer.current_structured_item(); minibuffer.message(item.kind .. '|' .. tostring(item.row) .. '|' .. item.value.value)",
        );

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "record|1|one");
    }

    #[test]
    fn shell_contract_routes_tree_buffer_keys() {
        let mut app = App::new().expect("app should initialize");
        let target = app.state_mut().workspace_mut().create_text_buffer("target");
        let mut root = crate::kernel::TreeNode::new("root", "Root");
        root.push_child(crate::kernel::TreeNode::with_linked_buffer(
            "child", "Child", target,
        ));
        let tree_buffer = app.state_mut().workspace_mut().create_buffer(
            "tree",
            crate::kernel::BufferContent::Tree(crate::kernel::TreeContent::new(vec![root])),
        );
        assert!(app.state_mut().workspace_mut().focus_buffer(tree_buffer));
        let shell: &mut dyn AppShell = &mut app;

        press_key(shell, KeyCodeRepr::Char('j'), Some('j'));

        let moved_view = shell.view();
        assert_eq!(moved_view.minibuffer_text, "Selected tree node child");
        assert_eq!(
            moved_view.selected_item,
            Some(crate::session::SessionSelectedItemView::TreeNode {
                id: "child".to_string(),
                label: "Child".to_string(),
                linked_buffer_id: Some(target.raw()),
            })
        );

        press_key(shell, KeyCodeRepr::Char('d'), Some('d'));

        let described_view = shell.view();
        assert_eq!(described_view.minibuffer_text, "Tree node child: Child");

        send_command(shell, "buffer.structured.current");

        let current_view = shell.view();
        assert_eq!(current_view.minibuffer_text, "Tree node child: Child");

        press_key(shell, KeyCodeRepr::Enter, None);

        let opened_view = shell.view();
        let SessionPaneNode::Leaf(pane) = opened_view.pane_tree else {
            panic!("expected leaf pane view");
        };
        assert_eq!(pane.buffer_name, "target");
        assert_eq!(
            opened_view.minibuffer_text,
            format!("Focused linked buffer {target}")
        );
    }

    #[test]
    fn shell_contract_routes_lua_tree_navigation() {
        let mut app = App::new().expect("app should initialize");
        let target = app.state_mut().workspace_mut().create_text_buffer("target");
        let mut root = crate::kernel::TreeNode::new("root", "Root");
        root.push_child(crate::kernel::TreeNode::with_linked_buffer(
            "child", "Child", target,
        ));
        let tree_buffer = app.state_mut().workspace_mut().create_buffer(
            "tree",
            crate::kernel::BufferContent::Tree(crate::kernel::TreeContent::new(vec![root])),
        );
        assert!(app.state_mut().workspace_mut().focus_buffer(tree_buffer));
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval buffer.next_structured_item(); local item = buffer.current_structured_item(); minibuffer.message(item.id .. '|' .. item.label)",
        );

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "child|Child");
    }

    #[test]
    fn shell_contract_routes_lua_current_structured_tree_node() {
        let mut app = App::new().expect("app should initialize");
        let target = app.state_mut().workspace_mut().create_text_buffer("target");
        let mut root = crate::kernel::TreeNode::new("root", "Root");
        root.push_child(crate::kernel::TreeNode::with_linked_buffer(
            "child", "Child", target,
        ));
        let tree_buffer = app.state_mut().workspace_mut().create_buffer(
            "tree",
            crate::kernel::BufferContent::Tree(crate::kernel::TreeContent::new(vec![root])),
        );
        assert!(app.state_mut().workspace_mut().focus_buffer(tree_buffer));
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval buffer.next_structured_item(); local item = buffer.current_structured_item(); minibuffer.message(item.kind .. '|' .. item.id .. '|' .. item.label)",
        );

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "tree_node|child|Child");
    }

    #[test]
    fn shell_contract_routes_lua_tree_open() {
        let mut app = App::new().expect("app should initialize");
        let target = app.state_mut().workspace_mut().create_text_buffer("target");
        let mut root = crate::kernel::TreeNode::new("root", "Root");
        root.push_child(crate::kernel::TreeNode::with_linked_buffer(
            "child", "Child", target,
        ));
        let tree_buffer = app.state_mut().workspace_mut().create_buffer(
            "tree",
            crate::kernel::BufferContent::Tree(crate::kernel::TreeContent::new(vec![root])),
        );
        assert!(app.state_mut().workspace_mut().focus_buffer(tree_buffer));
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval buffer.next_structured_item(); buffer.open_structured_item()",
        );

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "target");
        assert_eq!(
            view.minibuffer_text,
            format!("Focused linked buffer {target}")
        );
    }

    #[test]
    fn shell_contract_routes_lua_open_structured_item() {
        let mut app = App::new().expect("app should initialize");
        let target = app.state_mut().workspace_mut().create_text_buffer("target");
        let results = app
            .state_mut()
            .workspace_mut()
            .create_records_buffer("results");
        assert!(app.state_mut().workspace_mut().push_record_to_buffer(
            results,
            json!({"buffer_id": target.raw(), "label": "linked"})
        ));
        assert!(app.state_mut().workspace_mut().focus_buffer(results));
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "eval buffer.open_structured_item()");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "target");
        assert_eq!(
            view.minibuffer_text,
            format!("Focused linked buffer {target}")
        );
    }

    #[test]
    fn shell_contract_routes_lua_contextual_job_focus_output() {
        let mut app = App::new().expect("app should initialize");
        app.set_package_runner(TestPackageRunner {
            events: vec![PackageRunEvent::Record(json!({"value": "hello"}))],
            result: Ok(PackageRunResult { exit_code: 0 }),
        });
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "package.run echo default value=hello");
        send_command(
            shell,
            "eval local id = job.open(); buffer.focus(id); buffer.select_record(0); job.focus_output() ",
        );

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*pkg:echo default*");
        assert_eq!(
            view.minibuffer_text,
            "Focused output buffer 2 for job 1 (echo default)"
        );
    }

    #[test]
    fn shell_contract_surfaces_lua_current_buffer_for_text_buffer() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval local buf = buffer.current(); minibuffer.message(buf.kind .. '|' .. tostring(buf.text_line_count))",
        );

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "text|1");
    }

    #[test]
    fn shell_contract_routes_lua_browser_buffer_open() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval local id = buffer.open_browser('https://example.com'); buffer.focus(id)",
        );

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*browser:https://example.com*");
        assert_eq!(pane.buffer_kind, BufferKind::Browser);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Browser {
                url: Some("https://example.com".to_string()),
                title: None,
            }
        );
        assert_eq!(view.minibuffer_text, "Opened https://example.com");
    }

    #[test]
    fn shell_contract_routes_lua_browser_title_set() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval local id = buffer.open_browser('https://example.com'); buffer.set_browser_title(id, 'Example Domain')",
        );

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*browser:https://example.com*");
        assert_eq!(pane.buffer_kind, BufferKind::Browser);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Browser {
                url: Some("https://example.com".to_string()),
                title: Some("Example Domain".to_string()),
            }
        );
        assert_eq!(view.minibuffer_text, "Set browser title for 2");
    }

    #[test]
    fn shell_contract_routes_lua_current_browser_title_set() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval buffer.open_browser('https://example.com'); buffer.set_current_browser_title('Example Domain')",
        );

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*browser:https://example.com*");
        assert_eq!(pane.buffer_kind, BufferKind::Browser);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Browser {
                url: Some("https://example.com".to_string()),
                title: Some("Example Domain".to_string()),
            }
        );
        assert_eq!(view.minibuffer_text, "Set browser title for 2");
    }

    #[test]
    fn shell_contract_surfaces_lua_current_buffer_for_browser_buffer() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval buffer.open_browser('https://example.com'); buffer.set_current_browser_title('Example Domain'); local buf = buffer.current(); minibuffer.message(buf.kind .. '|' .. buf.browser_url .. '|' .. buf.browser_title)",
        );

        let view = shell.view();

        assert_eq!(
            view.minibuffer_text,
            "browser|https://example.com|Example Domain"
        );
    }

    #[test]
    fn shell_contract_routes_lua_browser_url_set() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval local id = buffer.open_browser('https://before.example'); buffer.set_browser_url(id, 'https://after.example')",
        );

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*browser:https://before.example*");
        assert_eq!(pane.buffer_kind, BufferKind::Browser);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Browser {
                url: Some("https://after.example".to_string()),
                title: None,
            }
        );
        assert_eq!(view.minibuffer_text, "Set browser url for 2");
    }

    #[test]
    fn shell_contract_routes_lua_current_browser_url_set() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval buffer.open_browser('https://before.example'); buffer.set_current_browser_url('https://after.example')",
        );

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*browser:https://before.example*");
        assert_eq!(pane.buffer_kind, BufferKind::Browser);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Browser {
                url: Some("https://after.example".to_string()),
                title: None,
            }
        );
        assert_eq!(view.minibuffer_text, "Set browser url for 2");
    }

    #[test]
    fn shell_contract_routes_lua_media_buffer_open() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval local id = buffer.open_media('./clip.mp4'); buffer.focus(id)",
        );

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*media:./clip.mp4*");
        assert_eq!(pane.buffer_kind, BufferKind::Media);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Media {
                source: Some("./clip.mp4".to_string()),
            }
        );
        assert_eq!(view.minibuffer_text, "Opened ./clip.mp4");
    }

    #[test]
    fn shell_contract_routes_lua_media_source_set() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval local id = buffer.open_media('./before.mp4'); buffer.set_media_source(id, './after.mp4')",
        );

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*media:./before.mp4*");
        assert_eq!(pane.buffer_kind, BufferKind::Media);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Media {
                source: Some("./after.mp4".to_string()),
            }
        );
        assert_eq!(view.minibuffer_text, "Set media source for 2");
    }

    #[test]
    fn shell_contract_routes_lua_current_media_source_set() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval buffer.open_media('./before.mp4'); buffer.set_current_media_source('./after.mp4')",
        );

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*media:./before.mp4*");
        assert_eq!(pane.buffer_kind, BufferKind::Media);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Media {
                source: Some("./after.mp4".to_string()),
            }
        );
        assert_eq!(view.minibuffer_text, "Set media source for 2");
    }

    #[test]
    fn shell_contract_surfaces_lua_current_buffer_for_media_buffer() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval buffer.open_media('./clip.mp4'); local buf = buffer.current(); minibuffer.message(buf.kind .. '|' .. buf.media_source)",
        );

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "media|./clip.mp4");
    }

    #[test]
    fn shell_contract_routes_lua_canvas_buffer_open() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval local id = buffer.open_canvas('playground'); buffer.focus(id)",
        );

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*canvas:playground*");
        assert_eq!(pane.buffer_kind, BufferKind::Canvas);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Canvas {
                name: Some("playground".to_string()),
            }
        );
        assert_eq!(view.minibuffer_text, "Opened playground");
    }

    #[test]
    fn shell_contract_routes_lua_canvas_name_set() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval local id = buffer.open_canvas('before'); buffer.set_canvas_name(id, 'after')",
        );

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*canvas:before*");
        assert_eq!(pane.buffer_kind, BufferKind::Canvas);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Canvas {
                name: Some("after".to_string()),
            }
        );
        assert_eq!(view.minibuffer_text, "Set canvas name for 2");
    }

    #[test]
    fn shell_contract_routes_lua_current_canvas_name_set() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval buffer.open_canvas('before'); buffer.set_current_canvas_name('after')",
        );

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "*canvas:before*");
        assert_eq!(pane.buffer_kind, BufferKind::Canvas);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Canvas {
                name: Some("after".to_string()),
            }
        );
        assert_eq!(view.minibuffer_text, "Set canvas name for 2");
    }

    #[test]
    fn shell_contract_surfaces_lua_current_buffer_for_canvas_buffer() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval buffer.open_canvas('playground'); local buf = buffer.current(); minibuffer.message(buf.kind .. '|' .. buf.canvas_name)",
        );

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "canvas|playground");
    }

    #[test]
    fn shell_contract_surfaces_lua_unknown_buffer_errors() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "eval buffer.focus(999)");

        let view = shell.view();

        assert!(
            view.minibuffer_text
                .contains("Lua error: unknown buffer id: 999")
        );
    }

    #[test]
    fn shell_contract_routes_lua_command_run() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "eval command.run('buffer.new cmd-notes')");

        let view = shell.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.buffer_name, "cmd-notes");
        assert_eq!(pane.buffer_kind, BufferKind::Text);
        assert_eq!(view.minibuffer_text, "Created cmd-notes");
    }

    #[test]
    fn shell_contract_surfaces_lua_command_list_metadata() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval local commands = command.list(); minibuffer.message(commands[1].name .. '|' .. commands[1].summary)",
        );

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "browser.open|Open a browser buffer");
    }

    #[test]
    fn shell_contract_surfaces_lua_command_get_metadata() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval local cmd = command.get('help'); minibuffer.message(cmd.name .. '|' .. cmd.usage)",
        );

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "help|Usage: :help [command]");
    }

    #[test]
    fn shell_contract_surfaces_lua_missing_command_as_nil() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(
            shell,
            "eval local cmd = command.get('missing'); minibuffer.message(tostring(cmd == nil))",
        );

        let view = shell.view();

        assert_eq!(view.minibuffer_text, "true");
    }

    #[test]
    fn shell_contract_routes_pane_resize_left_command() {
        let mut app = App::new().expect("app should initialize");
        let shell: &mut dyn AppShell = &mut app;

        send_command(shell, "pane.split.vertical");
        send_command(shell, "pane.resize.left");

        let view = shell.view();

        assert!(matches!(
            view.pane_tree,
            SessionPaneNode::Split {
                ratio_percent: 40,
                ..
            }
        ));
        assert_eq!(view.minibuffer_text, "Resized pane left");
    }
}
