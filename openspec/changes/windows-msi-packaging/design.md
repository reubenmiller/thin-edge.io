## Goal

Package thin-edge.io for Windows as an MSIX installer that registers `tedge-agent` and `tedge-mapper-c8y` as Windows Services using the multi-call binary pattern (`tedge run <service>`). The MSIX is produced via nfpm (the existing Linux packager, which supports MSIX) and integrated into the existing Windows CI pipeline.

## Decisions

### MSIX via nfpm, not MSI/WiX

Use MSIX as the package format, produced by nfpm. This reuses the existing packaging toolchain already used for Linux (`.deb`, `.rpm`, `.apk`) and avoids introducing WiX Toolset as an additional build dependency.

**Why MSIX over MSI**: MSIX is the current Windows packaging standard, supported natively by the Windows Package Manager (`winget`). It provides atomic installs, clean uninstalls, and built-in upgrade support without custom action scripting. MSIX packages run as full-trust apps when the `runFullTrust` capability is declared, which gives them unrestricted access to `C:\ProgramData\` — the same access a traditional MSI service would have.

**Trade-off**: MSIX service support (`desktop6:Extension`) requires Windows 10 version 2004 (build 19041) or later. Earlier Windows 10 releases and Windows Server 2016 are not supported. This is an acceptable constraint for a modern device management agent.

### Service declarations via desktop6 AppxManifest extensions

Windows Services are declared in `AppxManifest.xml` using the `desktop6` namespace (`http://schemas.microsoft.com/appx/manifest/desktop/windows10/6`). Each service maps to a `desktop6:Extension` block:

```xml
<Extensions>
  <desktop6:Extension Category="windows.service" EntryPoint="Windows.FullTrustApplication"
                       Executable="bin\tedge.exe">
    <desktop6:Service Name="tedge-agent" StartAccount="localSystem"
                      StartupType="auto" Arguments="run tedge-agent"/>
  </desktop6:Extension>
  <desktop6:Extension Category="windows.service" EntryPoint="Windows.FullTrustApplication"
                       Executable="bin\tedge.exe">
    <desktop6:Service Name="tedge-mapper-c8y" StartAccount="localSystem"
                      StartupType="auto" Arguments="run tedge-mapper c8y"/>
  </desktop6:Extension>
</Extensions>
```

The `Executable` path is relative to the package root. Both services use the single multi-call `tedge.exe` with different `Arguments`.

**Required capabilities**: `runFullTrust` (for full filesystem access) and `localSystemServices` (for `StartAccount="localSystem"`).

**nfpm MSIX and service declarations**: nfpm generates the `AppxManifest.xml` from its YAML config. If nfpm's MSIX support does not yet expose the `desktop6:Service` extension natively, a hand-authored `AppxManifest.xml` template can be supplied and nfpm instructed to use it verbatim. This is a known fallback path — the packaging YAML would reference the manifest as an override rather than generating one from scratch.

### Service account: LocalSystem

Register services under `LocalSystem`. Same rationale as MSI approach: eliminates permission failures during bring-up, conventional for device agents. Least-privilege hardening (NetworkService or dedicated account) is deferred.

### File layout

MSIX binaries are installed by Windows into `C:\Program Files\WindowsApps\{PackageFullName}\`. Application code cannot choose this path — it is OS-managed. All mutable data goes outside the package into `C:\ProgramData\tedge\`, which full-trust MSIX apps can read/write directly without virtualization.

Package contents (read-only, inside the MSIX):
- `bin\tedge.exe` — multi-call binary

Installed outside the package to `C:\ProgramData\tedge\` by a post-install script:
- `tedge.toml` — base configuration (explicit Windows paths, `NeverOverwrite`-equivalent)
- `sm-plugins\winget.ps1` — software management plugin

Runtime data directories (created on first service start if absent):
- `C:\ProgramData\tedge\data\` — persistent agent data
- `C:\ProgramData\tedge\log\` — log files
- `C:\ProgramData\tedge\tmp\` — in-progress operation staging

### Ship a pre-configured tedge.toml

A `tedge.toml` is placed at `C:\ProgramData\tedge\tedge.toml` by the post-install script. It explicitly sets the Windows path layout so it is self-documenting and stable across upgrades:

```toml
[data]
path = 'C:\ProgramData\tedge\data'

[logs]
path = 'C:\ProgramData\tedge\log'

[tmp]
path = 'C:\ProgramData\tedge\tmp'
```

The post-install script writes this file only if it does not already exist (preserving user edits on upgrade). These values match the compile-time defaults in `platform.rs` and pin them against future default changes.

### SM plugins shipped in the package

`winget.ps1` (from `configuration/contrib/sm-plugins/`) is placed at `C:\ProgramData\tedge\sm-plugins\winget.ps1` by the post-install script. This matches the path the plugin manager scans at runtime (`{config_root}\sm-plugins`), making winget-based software management available immediately after install.

### One binary, no per-service executables

The MSIX ships a single `tedge.exe`. Each service declaration in the manifest references it with different `Arguments`. No winsw, NSSM, or service wrapper is needed.

## Non-goals / deferred

- **MSI/WiX packaging**: deferred; MSIX is the primary format. An MSI could be added later for Windows Server 2016 / older Windows 10 compatibility if required.
- **`c8y-firmware-plugin` and `c8y-remote-access-plugin` as Windows Services**: deferred until those plugins are validated on Windows.
- **`tedge-watchdog`**: Linux-only service; no Windows equivalent planned.
- **Least-privilege service account**: NetworkService or a dedicated user is a future hardening item.
- **MSIX code-signing**: required for distribution via winget/Store; not a blocker for initial internal use (can be installed with `Add-AppxPackage -AllowUnsigned`).
- **GUI/wizard installer**: silent install is the primary path (`Add-AppxPackage` or `winget install`).
- **`tedge init` for Windows**: initial cloud connectivity configuration is out of scope; the package installs and starts services but does not configure certificates or cloud URLs.
- **Upgrade orchestration during OTA**: service restart during upgrade may interrupt an in-flight OTA operation — same trade-off as Linux `.deb`. No mitigation planned.

## Risks / trade-offs

- **Windows version floor**: `desktop6:Service` requires Windows 10 v2004 (build 19041+). Devices on older Windows 10 or Windows Server 2016 cannot use this package.
- **nfpm desktop6 service support**: nfpm's MSIX implementation may not expose `desktop6:Service` extension configuration natively. If so, a custom `AppxManifest.xml` template must be maintained alongside the nfpm YAML — this is a hand-off point to verify during implementation.
- **Post-install script trust**: MSIX custom actions (post-install PowerShell) require the package to be signed or the machine to have developer mode / sideloading enabled. For the post-install `tedge.toml` write, an alternative is to have the service write a default config on first run, removing the need for a post-install script entirely.
- **Upgrade config preservation**: unlike WiX `NeverOverwrite`, the post-install script must explicitly check for an existing `tedge.toml` before writing. If the script is not idempotent, upgrades can overwrite user configuration.

## Capabilities

### New Capabilities

- windows-msix-package
- windows-services

### Modified Capabilities

(none)
