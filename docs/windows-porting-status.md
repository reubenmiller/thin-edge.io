# Windows Porting Status

This document summarises the current state of the `feat/windows-port-phase1` branch,
cataloguing every skipped test, active workaround, and known gap.
It is intended as the basis for a prioritised plan to either fix the remaining items
or formally declare them out-of-scope for Windows.

---

## Overall Metrics (as of branch head)

| Metric | Value |
|--------|-------|
| Tests passing on Windows | **~2,370** |
| Tests skipped on Windows | **~110** |
| Tests failing on Windows | **0** |
| Commits in this port branch | 23 |

---

## Part 1 — Active Workarounds (Technical Debt)

These are in-place fixes that work but are suboptimal. They should be improved
before this branch is merged into `main`.

### 1.1 TOML Path Normalisation in Test Helper — `load_toml_str_with_warnings`
**Priority: Medium**

**Location:** `crates/common/tedge_config/src/tedge_toml/tedge_config_location.rs`

**Problem:** Windows paths embedded in TOML `"..."` strings (e.g.
`cert_path = "C:\Users\..."`) are misread as TOML escape sequences causing
parse errors. Our fix normalises `\` → `/` *inside the test-only
`load_toml_str_with_warnings` helper* before calling `toml::from_str`.

**Why it's hacky:**
- Only covers tests that go through `load_toml_str*`. Tests that write TOML to
  disk and have the binary read it back required individual `.replace('\\', "/")`
  fixups scattered across ~15 test files.
- The right fix is a shared utility that handles TOML-safe path formatting, or
  using TOML literal strings (`'C:\Users\...'`) where paths are embedded.

**Affected files requiring individual `.replace('\\', "/")`:**
`cli/mapper/cli.rs`, `tedge_mapper/src/{az,aws,c8y}/mapper.rs`,
`tedge_config_manager/src/tests.rs`, `tedge_file_config_plugin/tests/cli.rs`,
`tedge_file_log_plugin/tests/cli.rs`

---

### 1.2 Windows Service Manager Uses `sc.exe` CLI
**Priority: High**

**Location:** `crates/common/tedge_system_services/src/managers/windows_manager.rs`

**Problem:** The `WindowsServiceManager` drives `sc.exe` (the Windows Service
Control Manager command-line tool) to start/stop/enable/disable services. This
is an interim approach that:
- Spawns a child process per operation (slow, no structured error codes)
- Depends on `sc.exe` being in `PATH`
- Provides no rich error reporting (just string matching on exit code)

**Correct fix:** Use the `windows-service` crate (v0.7) to call SCM APIs
directly — see the architecture document for the implementation plan.

---

### 1.3 PKCS#11 Proxy Disabled on Windows
**Priority: High (medical device sector)**

**Location:**
- `crates/common/tedge-p11/src/lib.rs` — proxy module gated `#[cfg(unix)]`
- `crates/extensions/tedge-p11-server/src/main.rs` — gated `#![cfg(unix)]`

**Problem:** The PKCS#11 proxy (`tedge-p11-server`) uses Unix domain sockets
for IPC. On Windows, attempting to create a `SocketService` configuration
returns `Err("PKCS#11 socket proxy is not supported on Windows")`.

The `Direct` configuration (calling the cryptoki library directly) **does** work
on Windows.

**Correct fix:** Replace Unix sockets with Windows named pipes using the
`interprocess` crate (`\\.\pipe\tedge-p11-server`), which provides a unified
abstraction over both.

---

### 1.4 C8Y Remote Access Plugin Falls Back to `spawn_child`
**Priority: Medium**

**Location:** `plugins/c8y_remote_access_plugin/src/lib.rs`

**Problem:** The `TryConnectUnixSocket` command path connects to a Unix domain
socket (`/run/c8y-remote-access-plugin.sock`) to reuse a running server
instance. On Windows this always falls back to `spawn_child` — meaning a new
process is spawned per operation instead of reusing the server.

The plugin **does** work on Windows; it just loses the process-sharing
optimisation.

**Correct fix:** Replace the Unix socket IPC with a Windows named pipe
(same fix as 1.3), making the optimisation available on both platforms.

---

### 1.5 File Locking Uses Open-Handle Emulation
**Location:** `crates/common/flockfile/src/windows.rs`

**Priority: Low**

**Problem:** Unix advisory file locking (`flock`) is emulated on Windows by
keeping an open exclusive `File` handle. This is *mandatory* (not advisory) —
another process cannot open the file, which is stronger than Linux. However,
the implementation note warns:
> "this is an advisory-style emulation — it protects against well-behaved
> concurrent thin-edge processes but not against programs that ignore the lock"

The lock file is also deleted on `Drop`, which could race if the process crashes.

**Correct fix:** Acceptable as-is for Phase 1. If stronger guarantees are needed
for medical-device compliance, investigate `CreateMutex` (named system mutex)
as an alternative.

---

### 1.6 No Directory `fsync` After Atomic Writes
**Location:** `crates/common/tedge_utils/src/fs.rs`

**Priority: Low**

**Problem:** `atomically_write_file_sync` and `atomically_write_file_async`
skip the `sync_all()` call on the parent directory on Windows (Windows does not
allow opening directories for `sync` via `File::open`). NTFS provides sufficient
durability guarantees for atomic renames so this is safe, but it deviates from
the Linux behavior.

**Correct fix:** Acceptable as-is. Windows `MoveFileEx(MOVEFILE_REPLACE_EXISTING)` 
is already durable on NTFS without directory fsync.

---

### 1.7 File Permissions and Ownership Are No-ops
**Location:** `crates/common/tedge_utils/src/file.rs`, `atomic.rs`, etc.

**Priority: Medium**

**Problem:** All `change_user`, `change_group`, `change_mode` functions are
no-ops on Windows (they return `Ok(())` silently). This means:
- Private key files (`0o600`) are world-readable
- Config files are writable by any process running as the same user
- The `tedge write` helper cannot enforce ACL-based permissions

**Correct fix:** Use Windows ACLs (`windows-acl` crate or native Win32 API) to
set appropriate permissions. At minimum, restrict private keys to the installing
user. This is important for medical-device certification.

---

### 1.8 Diagnostic Plugin Executability Check is Extension-Based
**Location:** `crates/core/tedge/src/cli/diag/collect.rs` —
`path_is_executable_on_windows`

**Priority: Low**

**Problem:** On Linux, executability is determined by the `execute` bit.
On Windows, we check for `.exe`, `.bat`, `.cmd`, `.ps1`, `.com` extensions.
This means diagnostic plugins must have one of these extensions on Windows,
which differs from the Linux convention of extensionless scripts.

**Correct fix:** Document that Windows diagnostic plugins must use `.bat` or
`.ps1` extensions. Optionally update plugin scaffolding tools.

---

## Part 2 — Skipped Tests by Category

### 2.1 Unix Shell Scripts (bash/sh) — ~30 tests
**Can these be fixed for Windows?** Partially, with effort.

These tests create and execute `#!/bin/bash` or `#!/bin/sh` shell scripts.
On Windows, bash is not available (unless Git Bash is installed, but that's
not guaranteed in production).

**Skipped crates:**
- `tedge_log_manager` (9 tests) — `prepare()` creates a bash plugin script
- `tedge_config_manager` (12 tests) — same bash plugin pattern  
- `c8y_mapper_ext` (4 tests) — `create_custom_cmd()` with `#!/bin/sh`

**Action:** For the log/config manager plugin scripts, the fix is to provide
Windows-native test plugins (`.bat` or `.ps1`). A helper analogous to
`with_exec_permission` that creates platform-appropriate scripts would unblock
all these tests. This is **recommended** since it validates real Windows
plugin functionality.

---

### 2.2 Child Device IDs with Colons in Filesystem Paths — 7 tests
**Can these be fixed for Windows?** Yes, architectural change needed.

The C8y external ID format for child devices is `device-id:device:child-name`,
which contains colons. Colons are forbidden in Windows directory names.
When the mapper creates `operations/c8y/test-device:device:child1/`, it fails
on Windows with "invalid filename" (OS error 123).

**Affected tests:**
`create_firmware_operation_file_for_child_device`,
`create_device_profile_operation_file_for_child_device`,
`mapper_converts_log_upload_cmd_to_supported_op_and_types_for_child_device`,
`mapper_converts_smartrest_logfile_req_to_log_upload_cmd_for_child_device`,
`mapper_converts_config_cmd_to_supported_op_and_types_for_child_device`,
`mapper_dynamically_updates_supported_operations_for_child_device`,
`mapper_publishes_all_supported_operations_on_signal`

**Action:** Sanitise child device external IDs when used as filesystem paths
(replace `:` with `_` or URL-encode). This is a **production bug**, not just
a test issue — actual child devices will fail to have operations files created.
**Priority: High.**

---

### 2.3 Unix Signals (SIGTERM, SIGKILL, SIGUSR1) — 7 tests
**Can these be fixed for Windows?** No (by design).

POSIX signals do not exist on Windows. SIGTERM/SIGKILL are simulated in
`tedge_script_ext` by waiting out the timeout (no graceful termination).
SIGUSR1 (used to trigger supported-operations refresh) has no Windows
equivalent.

**Affected tests:**
`custom_operation_timeout_sigterm`,
`custom_operation_timeout_sigkill`,
`signal_determines_next_state`,
`error_messages_capture_script_killing_signal`,
`mapper_publishes_all_supported_operations_on_signal`

**Action:** For `SIGUSR1` (operations refresh), consider using a Windows
message, named event, or MQTT message as a Windows-specific trigger. For
SIGTERM/SIGKILL in script operations, the current "wait out timeout" fallback
is acceptable. Mark these as **intentionally Windows-incompatible**.

---

### 2.4 Linux Absolute Paths (`/etc/tedge`, `/var/`) in Tests — 12 tests
**Can these be fixed for Windows?** Yes, test refactoring.

Tests hardcode `/etc/tedge/...` paths or assert that Linux-style root paths
(`/flows`, `/etc/...`) are recognised as absolute. On Windows these resolve
to drive-relative paths (`C:\etc\...`) or cause assertion failures.

**Affected crates:**
`tedge_utils/paths.rs` (3 tests),
`tedge_mapper/custom/config.rs` (3 tests),
`tedge_mapper/custom/resolve.rs` (5 tests),
`tedge_config/tedge_config_location.rs` (2 tests),
`tedge_flows/src/config.rs` (10 tests)

**Action:** Refactor tests to use `TempTedgeDir` for path fixtures instead of
hardcoded Linux paths. This is straightforward but tedious. **Priority: Medium.**

---

### 2.5 TLS Error Chain Differences — 4 tests
**Can these be fixed for Windows?** Yes, improved error matching.

When a TLS connection is rejected, Linux surfaces `rustls::AlertDescription`
via `reqwest → hyper → rustls`. Windows delivers `WSAECONNABORTED` (OS error
10053) at the socket layer before the TLS alert is extracted.

**Affected tests:**
`acceptor_rejects_connection_without_certificate`,
`acceptor_rejects_untrusted_client_certificates` (axum_tls),
`server_rejects_unauthenticated_connections_if_configured`,
`downloader_error_shows_certificate_required_error_when_appropriate`

**Action:** Add Windows-specific error matching in `error_matching.rs` — check
for OS error 10053 or the Winsock connection abort error alongside the rustls
alert. The TLS *rejection itself* works correctly on Windows; only the
error-assertion chain differs. **Priority: Low.**

---

### 2.6 Filesystem Notification Event Differences — 5 tests
**Can these be fixed for Windows?** Yes, with test redesign.

`notify` uses `ReadDirectoryChangesW` on Windows, which does not emit
`AccessKind::Close(Write)` events (the Linux `IN_CLOSE_WRITE` equivalent).
Test helper `assert_rx_stream` has no timeout and would hang forever waiting
for events that never arrive.

**Affected tests:** All 5 tests in `tedge_utils::notify::tests`

**Action:**
1. Add a timeout to `assert_rx_stream` (e.g. 5 seconds) so tests fail fast
   rather than hanging.
2. Audit which events `ReadDirectoryChangesW` emits for each operation and
   update the expected event sets accordingly.
**Priority: Medium** (the notify crate is central to thin-edge.io's config
watching functionality on Windows).

---

### 2.7 `tedge` Binary Startup Issue on Windows — ~20 tests
**Can these be fixed for Windows?** Requires investigation.

All integration tests in `crates/core/tedge/tests/main.rs` and `tests/mqtt.rs`
that spawn the `tedge` binary are gated with `#[cfg(not(windows))]` because even
`tedge --help` exits non-zero on Windows. The exact startup failure has not been
diagnosed — it does not manifest as a compilation error and occurs before any
user code in `main()` is reached.

**Suspected causes:**
- A `lazy_static!` or `Lazy<...>` initialisation that calls Unix-specific code
- The `tokio` runtime initialiser behaving differently
- A dependency's `build.rs` producing Windows-incompatible output

**Action:** Run the `tedge` binary directly on a Windows machine (or in a
Windows Docker container) with `RUST_BACKTRACE=1` to capture the startup panic
stack trace. **Priority: Critical** — this blocks the primary thin-edge.io
binary from being usable on Windows.

---

### 2.8 Diagnostic Plugin Execution — 3 tests
**Can these be fixed for Windows?** Yes, with test changes.

Tests in `cli::diag::collect` create plugin files using `with_exec_permission`
(which on Linux sets the execute bit). On Windows, `with_exec_permission` is a
no-op — the file has no `.exe/.bat/.ps1` extension so Windows rejects it as
non-executable.

**Action:** Create Windows-native test plugins as `.bat` files in
`with_exec_permission` or supply platform-specific test plugin files.

---

### 2.9 `invalid character '?' in filename` Test — 1 test
**Can this be fixed for Windows?** Partially (skip or change fixture).

`get_operations_skips_operations_with_invalid_names_and_content` tests that
files named `.command?` are ignored by the operation loader. On Windows, `?`
is an illegal filename character — `std::fs::rename` fails before the test
logic even runs.

**Action:** Use a different "invalid" character for the Windows case (e.g. a
leading dot `.command` which is still valid on Windows but would be filtered
by `is_valid_operation_name`). The test behaviour is valuable on Windows and
should be made cross-platform. **Priority: Low.**

---

### 2.10 Symlinks Require Elevated Privileges — 6 tests
**Can these be fixed for Windows?** Conditionally.

Creating symlinks on Windows requires either:
- Developer Mode (available on Windows 10 v1703+)
- UAC elevation (not acceptable for normal operation)

**Affected tests:** Various symlink tests in `tedge_utils`, `axum_tls`, `paths.rs`

**Action:** In CI, enable Developer Mode on the Windows runner (one-line
GitHub Actions step). In production, document that symlinks need Developer Mode
or avoid symlink-dependent features on Windows. **Priority: Low for CI, Medium
for production.**

---

### 2.11 RFC3339 Timestamps in Log Filenames — Workaround In Place
**Location:** `crates/core/plugin_sm/src/operation_logs.rs`

**Status:** Fixed with `#[cfg(windows)]` sanitisation.

RFC3339 timestamps contain `:` (e.g. `12:34:56`) which are illegal in Windows
filenames. On Windows, `new_log_file()` now replaces `:` with `-`. The
`remove_outdated_logs` regex was updated to match both `:` and `-` separators.

**No further action needed** but note that existing Windows deployments upgraded
from a Linux-generated log dir will have both `:` and `-` formatted log files
until they rotate.

---

## Part 3 — Functionality Not Available on Windows

These items are **intentionally out of scope** for Windows — either because
they are Linux-specific by design, or because the Windows equivalent requires
significant architectural work beyond Phase 1.

| Feature | Reason | Alternative |
|---------|--------|-------------|
| `tedge-apt-plugin` | Debian APT is Linux-only | Use Windows package managers (winget, Chocolatey) |
| systemd watchdog | systemd doesn't exist on Windows | Windows SCM heartbeat / event log |
| Unix socket-based PKCS#11 proxy | Named pipes not yet implemented | Use `Direct` cryptoki config |
| `tedge completions bash/zsh/fish` | These shells not standard on Windows | Add PowerShell completion support |
| `/etc/ssl/certs` CA bundle | Windows uses system cert store | `reqwest` with `rustls-tls-native-roots` already handles this |
| `openrc` / BSD `service(8)` | Not applicable on Windows | Windows SCM (`windows-service` crate) |

---

## Part 4 — Prioritised Action Plan

### P0 — Critical (blocks basic Windows usage)

1. **Diagnose `tedge` binary startup failure** (§2.7)
   - Run `tedge --help` on Windows with `RUST_BACKTRACE=1`
   - Bisect which `lazy_static!` or init code panics
   - Expected effort: 0.5 days investigation + 0.5–2 days fix

### P1 — High (production correctness)

2. **Child device colon paths in operations directories** (§2.2)
   - Sanitise external IDs for filesystem paths: `test-device:device:child1` → `test-device_device_child1`
   - Update operation path construction in `c8y_mapper_ext`
   - Expected effort: 2 days

3. **File permissions and ownership (Windows ACLs)** (§1.7)
   - Implement `set_permissions` using `windows-acl` crate for private key files
   - At minimum restrict `device-certs/tedge-private-key.pem` to owner-only
   - Expected effort: 3 days

4. **Windows Service Manager via `windows-service` crate** (§1.2)
   - Replace `sc.exe` CLI calls with direct SCM API calls
   - Implement proper `service_main` entry point for SCM-spawned execution
   - Expected effort: 3 days

### P2 — Medium (feature completeness)

5. **Named pipe IPC for PKCS#11 proxy** (§1.3, §1.4)
   - Replace `tedge-p11-server` Unix sockets with Windows named pipes
   - Unblocks hardware security module use on Windows (critical for medical devices)
   - Expected effort: 5 days

6. **Filesystem notification event mapping** (§2.6)
   - Add timeout to `assert_rx_stream` test helper
   - Map `ReadDirectoryChangesW` event types to `FsEvent` correctly
   - Expected effort: 1–2 days

7. **Linux-path test fixtures** (§2.4)
   - Refactor 20+ tests to use `TempTedgeDir` rather than `/etc/tedge` hardcodes
   - Expected effort: 2 days

8. **Unix shell script plugins** (§2.1)
   - Add Windows-native (`.bat`/`.ps1`) test plugin variants
   - Update `with_exec_permission` to create platform-appropriate scripts
   - Expected effort: 2 days

### P3 — Low (polish / completeness)

9. **TLS error chain matching on Windows** (§2.5) — 1 day
10. **Symlinks in CI via Developer Mode** (§2.10) — 0.5 days
11. **Invalid filename test with cross-platform fixture** (§2.9) — 0.5 days
12. **SIGUSR1 alternative signal mechanism for Windows** (§2.3) — 2 days
13. **`flockfile` named mutex** (§1.5) — 1 day

---

## Part 5 — Workaround Inventory (for future cleanup)

| File | Workaround | Fix needed |
|------|-----------|------------|
| `tedge_config_location.rs` | `replace('\\', "/")` on TOML strings in test helper | Proper TOML path serialisation utility |
| 15 test files | Inline `replace('\\', "/")` for TOML path embedding | Shared `toml_path()` helper or TOML literal strings |
| `windows_manager.rs` | `sc.exe` CLI for service management | `windows-service` crate |
| `flockfile/windows.rs` | Open handle as advisory lock | Named system mutex |
| `tedge-p11/lib.rs` | `SocketService` returns error on Windows | Named pipe implementation |
| `fs.rs` | Skip directory `fsync` on Windows | Acceptable as-is (NTFS is durable) |
| `file.rs`, `atomic.rs` | `chown`/`chmod` are no-ops | Windows ACL implementation |
| `operation_logs.rs` | RFC3339 `:` → `-` in filenames | Acceptable as-is |
| `c8y_remote_access_plugin` | `TryConnectUnixSocket` always uses `spawn_child` | Named pipe server |
| `tests/main.rs` + `tests/mqtt.rs` | `#[cfg(not(windows))]` on entire module | Fix `tedge` binary startup |
