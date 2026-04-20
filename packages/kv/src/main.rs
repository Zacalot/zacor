use std::collections::HashMap;

zacor_package::include_manifest!();

#[cfg(not(target_family = "wasm"))]
fn main() {
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    let args: Vec<String> = std::env::args().collect();

    // Service mode: kv --listen=:<port>
    if let Some(listen_arg) = args.iter().find(|a| a.starts_with("--listen=")) {
        let addr = listen_arg.trim_start_matches("--listen=");
        let path = zr_kv::kv_data_file().unwrap_or_else(|e| {
            eprintln!("kv: {e}");
            std::process::exit(1);
        });
        let flush_path = path.clone();

        zacor_package::service_loop(
            addr,
            move || {
                let store = zr_kv::load_store(&path);
                let state = Arc::new(Mutex::new((store, flush_path.clone())));

                // Background flush thread
                let flush_state = Arc::clone(&state);
                thread::spawn(move || loop {
                    thread::sleep(Duration::from_secs(30));
                    let guard = flush_state.lock().unwrap();
                    let _ = zr_kv::save_store(&guard.1, &guard.0);
                });

                state
            },
            |state: &mut Arc<Mutex<(HashMap<String, String>, std::path::PathBuf)>>, invoke| {
                let mut guard = state.lock().unwrap();
                zr_kv::service_handler(&mut guard.0, invoke)
            },
        );
    }

    // Command mode: use protocol() with typed dispatch
    std::process::exit(zacor_package::protocol(
        "kv",
        |ctx| -> Result<i32, String> {
            let records = match ctx.command() {
                "set" => zr_kv::cmd_set(&ctx.args()?)?,
                "get" => zr_kv::cmd_get(&ctx.args()?)?,
                "list" => zr_kv::cmd_list(&ctx.args()?)?,
                "delete" => zr_kv::cmd_delete(&ctx.args()?)?,
                other => return Err(format!("unknown command: {other}")),
            };
            ctx.emit_all(records)?;
            Ok(0)
        },
    ));
}

// Under wasm, the module has no TCP listener and no threads. The daemon
// owns the service port and drives INVOKEs down the wasm stdin pipe.
// Both command-mode (single INVOKE, stdin EOF, exit) and service-mode
// (many INVOKEs until daemon closes stdin) flow through the same loop —
// stdin EOF is the only termination signal.
#[cfg(target_family = "wasm")]
fn main() {
    zacor_package::service_loop_stdin(
        || -> (std::path::PathBuf, HashMap<String, String>) {
            let path = zr_kv::kv_data_file().expect("kv data file");
            let store = zr_kv::load_store(&path);
            (path, store)
        },
        |state, invoke| {
            let records = zr_kv::service_handler(&mut state.1, invoke);
            // Flush-on-write: kv.json is tiny and durability matters more
            // than write amplification for a proof-of-concept service.
            let _ = zr_kv::save_store(&state.0, &state.1);
            records
        },
    );
}
