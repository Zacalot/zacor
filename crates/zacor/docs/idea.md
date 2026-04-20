# zacor

> Install once, type `zr` forever.

**zacor** is a cross-platform CLI core written in Rust that manages and runs modular command-line tools under a single unified command: `zr`.

It is both a **dispatcher** (routing `zr <package>` to the right tool) and a **package manager** (installing, updating, and removing packages). Packages are self-contained programs that plug into the `zr` namespace — they can be written in any language.

---

## Names

- **Project / package name:** `zacor`
- **Binary:** `zr`
- **Author:** zacalot
- **Repository:** `zacalot/zacor`

The package name `zacor` is used everywhere public-facing: crates.io, Homebrew, apt, scoop, snap. The binary name `zr` is what you type. This separation avoids the short-name collision problem that plagues tools like `fd` (shipped as `fdfind` on Debian) and `bat` (shipped as `batcat`).

In Cargo.toml:

```toml
[package]
name = "zacor"

[[bin]]
name = "zr"
path = "src/main.rs"
```

`zr --version` prints `zacor x.y.z`.

---

## What zacor is

**A CLI core.** `zr` is the foundation that everything else plugs into. On its own it does very little — its job is to find, manage, and execute packages.

**A package manager.** `zr` can install, update, list, and remove packages. Packages come from a registry, a git repository, or a local path.

**A namespace.** Every package lives under the `zr` prefix. Instead of remembering dozens of scattered scripts and binaries, everything is `zr <something>`.

**Cross-platform.** Written in Rust, ships as a single static binary. Works on Linux, macOS, and Windows with identical behavior.

## What zacor is not

**Not a shell.** It dispatches to programs; it doesn't replace bash/zsh/fish/powershell.

**Not a task runner.** Tools like `just` and `make` read recipe files per-project. `zr` manages standalone tools that work anywhere.

**Not opinionated about package languages.** A package can be a Rust binary, a Python script, a shell script, a Go binary — anything executable.

---

## How it works

### Dispatching

```
zr <package> [args...]
```

`zr` finds the package and executes it, forwarding all arguments, stdin, stdout, stderr, and the exit code transparently. The user's experience is as if they ran the package directly — `zr` just resolves where it lives.

### Package management

```
zr install <source>       # install a package
zr update <package>        # update a package
zr update                  # update all packages
zr remove <package>        # remove a package
zr list                    # list installed packages
```

Packages are installed into a managed directory. `zr` tracks what's installed, where it came from, and what version it is.

### Package sources

Packages can be installed from:

- **A registry** (a central index of published packages)
- **A git repository** (`zr install github.com/user/repo`)
- **A local path** (`zr install ./my-tool`)

The specifics of the registry format, resolution strategy, and build process are implementation details to be designed separately.

### Package discovery

When `zr <package>` is invoked, the resolution order is:

1. **Built-in commands** — core commands like `install`, `update`, `remove`, `list`, `help` that are part of `zr` itself
2. **Installed packages** — executables managed by `zr` in its packages directory
3. **Not found** — error with a fuzzy suggestion ("did you mean?")

### Two tiers of packages

**Built-in packages** are compiled into the `zr` binary. These are meta-commands for managing `zr` itself (install, update, list, etc.) and could optionally include a small set of core utilities.

**External packages** are standalone executables managed by `zr`. They can be written in any language. `zr` stores them in a platform-appropriate directory and invokes them as child processes.

Built-in commands take precedence over external packages with the same name.

---

## Design principles

**The core is small.** `zr` itself should be fast to install, fast to start, and minimal in scope. All interesting functionality lives in packages.

**Packages are just programs.** Any executable can be a package. No SDK, no framework, no plugin API required. If it runs, it's a package.

**One command to remember.** The entire point is that `zr` is the only command a user needs to memorize. Everything else is discoverable from there via `zr list`, `zr help <package>`, and tab completion.

**Cross-platform by default.** Every design decision must work on Linux, macOS, and Windows. No shell-specific behavior, no symlink tricks that break on Windows, no Unix-only assumptions.

**Instant dispatch.** The overhead `zr` adds before a module starts executing should be negligible — ideally under 1ms. Config parsing, module resolution, and process spawning must be fast.

---

## Example packages

To illustrate the kind of tools that could exist as `zr` packages. These are **examples, not commitments** — the package ecosystem is open-ended and user-driven.

**Dev workflow:**
- `zr new <template>` — scaffold a project from a template
- `zr clean` — remove build artifacts (node_modules, target/, .cache, etc.)
- `zr env` — dump environment info for debugging
- `zr ports` — show what's listening on the network

**Git shortcuts:**
- `zr wip` — stage everything and commit as "WIP"
- `zr pr` — open a pull request from the terminal

**Quick utilities:**
- `zr uuid` — generate a UUID
- `zr b64 <encode|decode>` — base64 operations
- `zr json` — pretty-print JSON from stdin
- `zr hash <file>` — compute file hashes

**System management:**
- `zr sync` — sync dotfiles
- `zr setup` — bootstrap a new machine
- `zr update` — update all zr packages

Any of these could be a 10-line shell script or a compiled Rust binary. `zr` doesn't care — it just needs to find and run them.

---

## Package conventions

Packages are not required to follow any particular interface. However, the following conventions enable a richer experience:

- **`--help`** — packages should handle this flag to describe themselves
- **Exit codes** — 0 for success, non-zero for failure
- **One-line description** — a way for packages to provide a short description that `zr list` can display (mechanism TBD — could be a sidecar file, a flag like `--zr-describe`, or metadata in the registry)
- **No global state assumptions** — packages should not assume they're the only thing running or that the user's shell is configured a particular way

---

## Open questions

These are decisions that need to be made during design and implementation:

- **Registry design:** What does the central package registry look like? Is it a git repo (like Homebrew taps), a web service, or something else?
- **Package distribution:** How are packages distributed? Source + build? Prebuilt binaries per platform? Both?
- **Versioning:** How are package versions tracked and resolved? Semver? Git tags? Something simpler?
- **Namespacing:** Can packages be grouped? (e.g., `zr git wip` vs `zr wip`)
- **Aliases:** Can users define shortcuts? (e.g., `zr b` → `zr build`)
- **Shell completions:** How does tab completion work when the set of packages is dynamic?
- **Configuration:** Does `zr` have a config file? What goes in it?
- **Security:** How are modules trusted? Is there signing, checksums, a review process?
- **Self-update:** Can `zr` update itself?

---

## Summary

`zr` is a small, fast core that gives you a single command to manage and run a personal ecosystem of CLI tools. The core handles dispatch and package management. Everything else is a package.
