# Tasks

## Investigate nfpm MSIX service support

- [x] Install nfpm and attempt to produce a minimal MSIX with a `desktop6:Service` extension to confirm whether nfpm exposes this natively or requires a custom `AppxManifest.xml` template
  - **Finding**: nfpm MSIX support is alpha and does not expose `desktop6:Service` declarations; custom manifest overrides are not supported. Pivoted to `makeappx.exe` (available on `windows-latest` GitHub Actions runners via the Windows SDK) with a hand-authored `AppxManifest.xml`.

## AppxManifest.xml

- [x] Create `configuration/package_manifests/windows/AppxManifest.xml` with package identity placeholders (`${VERSION}`, `${PUBLISHER}`)
- [x] Set `MinVersion="10.0.19041.0"` (Windows 10 v2004) in the `TargetDeviceFamily` element
- [x] Declare `runFullTrust` and `localSystemServices` restricted capabilities
- [x] Add `desktop6:Extension` for `tedge-agent` service (`tedge.exe`, `Arguments="run tedge-agent"`, `StartupType="auto"`, `StartAccount="localSystem"`)
- [x] Add `desktop6:Extension` for `tedge-mapper-c8y` service (`tedge.exe`, `Arguments="run tedge-mapper c8y"`, `StartupType="auto"`, `StartAccount="localSystem"`)

## nfpm MSIX package manifest

- [x] Create packaging script `configuration/package_scripts/windows/package.ps1` using `makeappx.exe` (replaces nfpm for MSIX)
- [x] Substitute `${VERSION}` and `${PUBLISHER}` placeholders into `AppxManifest.xml` at build time
- [x] Include `bin\tedge.exe` from `target\release\tedge.exe`
- [x] Include `sm-plugins\winget.ps1` from `configuration\contrib\sm-plugins\winget.ps1`

## Post-install bootstrap script

- [x] Create `configuration/package_scripts/windows/postinstall.ps1`
- [x] Create `C:\ProgramData\tedge\` and subdirectories (`data`, `log`, `tmp`, `sm-plugins`) if absent
- [x] Write `C:\ProgramData\tedge\tedge.toml` with Windows path defaults only if the file does not already exist
- [x] Write `C:\ProgramData\tedge\sm-plugins\winget.ps1` from the package-bundled copy only if the file does not already exist
- [x] First-run init also embedded in the `tedge` binary (`crates/core/tedge/src/cli/windows_init.rs`): called automatically before any `tedge run <service>` dispatch on Windows, so the service bootstraps itself on first start without requiring a separate post-install script step

## Config dir fix

- [x] Fix `get_config_dir()` in `tedge_config_location.rs` to return `platform::config_root()` on Windows (`C:\ProgramData\tedge`) instead of the Linux default `/etc/tedge`
- [x] Fix `TEdgeConfigLocation::default()` to use `platform::config_root()` on all platforms

## Build pipeline

- [x] Add a release build step to `ci-windows.yml`: `cargo build --package tedge --locked --release`
- [x] Add version detection step to `ci-windows.yml` (from git tag or Cargo.toml)
- [x] Add packaging step calling `package.ps1` with version and binary path
- [x] Upload the produced `.msix` as a CI artifact named `tedge-windows-msix`
- [x] `makeappx.exe` is available on `windows-latest` runners without additional installation

## Manual smoke test (sideloaded install)

- [ ] Verify `Add-AppxPackage -AllowUnsigned .\tedge.msix` succeeds on Windows 10 v2004+
- [ ] Verify `sc query tedge-agent` and `sc query tedge-mapper-c8y` show `RUNNING` after install
- [ ] Verify `C:\ProgramData\tedge\tedge.toml` exists with correct content after fresh install
- [ ] Verify `C:\ProgramData\tedge\sm-plugins\winget.ps1` exists after fresh install
- [ ] Verify `sc stop tedge-agent` and `sc start tedge-agent` work correctly
- [ ] Verify `C:\ProgramData\tedge\tedge.toml` is NOT overwritten when upgrading over an existing install
- [ ] Verify `sc query tedge-agent` returns service-not-found after `Remove-AppxPackage`
- [ ] Verify `C:\ProgramData\tedge\` remains on disk after uninstall

## Robot Framework tests

- [ ] Add a Robot Framework test that installs the MSIX and verifies both services reach `RUNNING` state
- [ ] Add a Robot Framework test that verifies `tedge.toml` is created with correct path defaults on fresh install
- [ ] Add a Robot Framework test that verifies `tedge.toml` is preserved when the MSIX is upgraded
- [ ] Add a Robot Framework test that verifies both services are unregistered after uninstall
- [ ] Add a Robot Framework test that verifies `C:\ProgramData\tedge\` is preserved after uninstall
