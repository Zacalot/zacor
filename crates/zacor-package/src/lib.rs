//! Package SDK for zacor/zr packages.
//!
//! This crate owns runtime, protocol, IO, and build-time integration
//! helpers. Domain libraries like `zr-expr` stay separate so packages can
//! depend on focused capabilities without growing the SDK surface.

extern crate self as zacor_package;

mod data;
mod input;
pub mod io;
pub mod parse;
pub mod protocol;
pub mod render;
mod run;
mod runtime;
pub mod skills;

pub use input::{parse_field_list, parse_records};

pub use data::{ensure_data_dir, ensure_data_dir_at};

/// Normalize a path to a string with forward slashes (consistent across platforms).
pub fn path_str(p: &std::path::Path) -> String {
    p.display().to_string().replace('\\', "/")
}
pub use run::{protocol, Context, FromArgs};
#[cfg(not(target_family = "wasm"))]
pub use run::service_loop;
#[cfg(target_family = "wasm")]
pub use run::service_loop_stdin;

pub use serde;
pub use serde::Serialize;
pub use serde_json;
pub use serde_json::json;

/// Expands to the standard `pub mod args` that includes the build-generated
/// arg structs. Use this in lib.rs.
#[macro_export]
macro_rules! include_args {
    () => {
        pub mod args {
            include!(concat!(env!("OUT_DIR"), "/args.rs"));
        }
    };
}

/// Embeds the generated package manifest as a wasm custom section
/// (`zacor_manifest`). No-op on native. Must be invoked exactly once in
/// the bin crate (lib crates consumed by other bins must NOT emit it, or
/// the linker will concatenate manifests into a single section and the
/// reader will see duplicate yaml fields). `single_command!` and
/// `commands!` inject this automatically; packages with a hand-rolled
/// `main.rs` should call it explicitly.
#[macro_export]
macro_rules! include_manifest {
    () => {
        #[doc(hidden)]
        mod __zacor_manifest_embed {
            include!(concat!(env!("OUT_DIR"), "/manifest.rs"));
        }
    };
}

/// Generates `fn main()` for a single-command package. The closure receives
/// `&mut Context` and returns `Result<i32, String>`.
///
/// ```ignore
/// zacor_package::single_command!("echo", |ctx| {
///     let args = ctx.args::<zr_echo::args::DefaultArgs>()?;
///     let record = zr_echo::echo(args)?;
///     ctx.emit_record(&record)?;
///     Ok(0)
/// });
/// ```
#[macro_export]
macro_rules! single_command {
    ($pkg:expr, |$ctx:ident| { $($body:tt)* }) => {
        ::zacor_package::include_manifest!();
        fn main() {
            std::process::exit(::zacor_package::protocol($pkg, |$ctx| -> Result<i32, String> {
                $($body)*
            }));
        }
    };
}

/// Generates `fn main()` for a multi-command package. Maps command names to
/// handler functions, dispatches via `ctx.command()`, and emits results with
/// `ctx.emit_all()`.
///
/// Annotations:
/// - `[input]` — passes `ctx.input()` as second argument
/// - `[exit]` — handler returns `Result<(Vec<Value>, i32), String>`; uses
///   the `i32` as exit code
///
/// ```ignore
/// zacor_package::commands!("rand", {
///     "int"     => zr_rand::cmd_int,
///     "pick"    => zr_rand::cmd_pick [input],
///     "check"   => zr_rand::cmd_check [exit],
/// });
/// ```
#[macro_export]
macro_rules! commands {
    ($pkg:expr, { $( $cmd:literal => $fn:path $( [ $ann:ident ] )? ),* $(,)? }) => {
        ::zacor_package::include_manifest!();
        fn main() {
            std::process::exit(::zacor_package::protocol($pkg, |ctx| -> Result<i32, String> {
                match ctx.command() {
                    $(
                        $cmd => $crate::commands!(@call ctx, $fn $(, $ann )? )
                    ),*,
                    other => return Err(format!("unknown command: {other}")),
                }
            }));
        }
    };

    // Call: no annotation — standard call, return Ok(0)
    (@call $ctx:ident, $fn:path) => {{
        let records = $fn(&$ctx.args()?)?;
        $ctx.emit_all(records)?;
        Ok(0)
    }};

    // Call: [input] — pass ctx.input() as second arg
    (@call $ctx:ident, $fn:path, input) => {{
        let records = $fn(&$ctx.args()?, $ctx.input())?;
        $ctx.emit_all(records)?;
        Ok(0)
    }};

    // Call: [exit] — handler returns (Vec<Value>, i32)
    (@call $ctx:ident, $fn:path, exit) => {{
        let (records, exit_code) = $fn(&$ctx.args()?)?;
        $ctx.emit_all(records)?;
        Ok(exit_code)
    }};
}
