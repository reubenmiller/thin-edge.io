## Goal

Add a Windows software management plugin backed by `winget` that lets thin-edge install, remove, and list application packages on Windows devices via the standard [Software Management Plugin API](docs/src/references/software-management-plugin-api.md). This fills the gap left by the Linux-only `apt` plugin and makes software management operational on Windows without any extra runtime components.

## Decisions

### PowerShell script, not a Rust binary

Implemented as a `.ps1` script dropped into `C:\ProgramData\tedge\sm-plugins\` rather than a Rust crate compiled into the multi-call `tedge` binary.

**Why**: The plugin system already treats `.ps1` files as first-class executables on Windows (extension stripping, interpreter launch — see `plugin_manager.rs`). The main argument for Rust would be zero extra runtime dependencies, but PowerShell 5.1 is shipped with every supported Windows version (10 / 11 / Server 2016+), so it is not an additional dependency in practice. The `Microsoft.WinGet.Client` PowerShell module provides structured object output (not screen-scraped text), making the implementation far more robust than shelling out to `winget` and parsing its human-readable CLI. winget's plain CLI output is localised and has changed across versions; the module API is versioned and stable.

A Rust implementation would need to either shell out to `winget.exe` and parse its output (fragile) or use WinRT/COM bindings (`windows-rs`) to call the Package Manager API directly — significant complexity for a plugin that by design runs as a child process anyway.

**Alternatives rejected**: Rust in multi-call binary — correct architectural fit but the COM/WinRT surface is large, the parser approach is brittle, and the plugin would need recompiling whenever winget behaviour changes. A `.bat` script was considered but lacks structured object access to winget results.

### Implement prepare and finalize (prepare runs source refresh)

`prepare` runs `winget source update` before a sequence of operations, equivalent to `apt-get update` in the Debian plugin. This ensures package metadata is current before installs. `finalize` is a documented no-op — winget has no transaction or post-install cache to clean up.

**Why**: Skipping `prepare` would mean the plugin always operates on potentially stale source metadata. `winget source update` is idempotent and fast (it is a background operation on modern winget). The sm-agent contract requires both commands to be present even if they do nothing, so `finalize` must be implemented regardless.

### Implement list, install, remove — skip update-list

The `update-list` command is optional per the plugin API: if a plugin returns exit code `1`, the sm-agent falls back to individual `install`/`remove` calls. Implementing `update-list` requires careful stdin parsing and adds complexity with little benefit for an initial version — the sm-agent fallback path is the correct approach here.

### install supports both winget source and local file (--file)

When `install` is called with `--file <path>`, the plugin installs the package from the local path using `winget install --silent <path>` rather than resolving via `--id` from a winget source. This is the primary mechanism for cloud-hosted binaries: the cloud operator provides a URL, tedge-agent downloads the file to a local temp path, then invokes the plugin with `--file`. The `--id`/`--module-version` arguments are still accepted alongside `--file` (for logging and verification) but the file path takes precedence as the install source.

**Why**: The plugin API explicitly supports this pattern — it is how large or proprietary installers that are not in a public winget source are distributed. Without `--file` support, the plugin would only work for packages available in configured winget sources, excluding any privately hosted software.

### winget only — Windows Update is a separate plugin

Windows Update (WU) patches the OS, updates drivers, and applies security fixes. winget installs user-space application packages. They serve different purposes, have different operational semantics (WU may require reboots and has approval/deferral policies), and are consumed by different device management workflows. A separate `windows-update` plugin can be added later without affecting this one.

### Package identity uses winget package ID, not display name

winget's canonical identifier for a package is its `Id` field (e.g. `Microsoft.VisualStudioCode`), not its display name. The plugin treats the `NAME` argument in `install`/`remove` commands as a winget package ID and passes it to `--id`. Display names are ambiguous (multiple packages can share a name); IDs are unique within a source.

**Why**: Using display names would require fuzzy matching and could silently install the wrong package. The cloud operator specifying the software module is expected to use the winget ID.

## Non-goals / deferred

- **update-list command**: optional per spec; sm-agent handles fallback automatically.
- **Windows Update plugin**: separate concern, different API surface, separate future change.
- **Winget source management**: adding/removing sources is out of scope; the plugin uses whatever sources are configured on the device.
- **Concurrent version management**: winget does not support multiple installed versions of the same package; the plugin makes no attempt to handle this.
- **Rust implementation**: deferred unless PowerShell becomes an unsupported dependency or a native binary is required for distribution reasons.

## Risks / trade-offs

- **`Microsoft.WinGet.Client` module availability**: The module must be installed separately (`Install-Module Microsoft.WinGet.Client`). It is not in the Windows box. The plugin should check for it at startup (`list` command) and return exit code `1` with a clear message if absent, rather than failing silently mid-operation.
- **winget requires user context for some operations**: winget installs per-machine vs per-user packages differently. Running as SYSTEM (the typical service account for tedge-agent) can cause winget to behave unexpectedly for per-user packages. The plugin should use `--scope machine` where available to force machine-wide installation.
- **Elevated privileges**: Installing packages typically requires admin rights. tedge-agent already runs with elevated privileges on Windows, so this should not be an issue in practice, but the plugin should surface clear errors if it encounters UAC failures.
- **Output parsing**: Even with the PowerShell module, version strings returned by `list` must be tab-separated per the plugin API. The module returns structured objects so this is straightforward, but version fields may be empty for some packages (e.g. MS Store apps).

## Capabilities

### New Capabilities
- windows-sm-winget
