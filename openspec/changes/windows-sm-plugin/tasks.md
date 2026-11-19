# Tasks

## Script skeleton

- [x] Create `configuration/contrib/sm-plugins/winget.ps1` with argument parsing for list/prepare/finalize/install/remove/update-list
- [x] Exit with code `1` and a usage message for any unrecognised command argument

## Prerequisites check

- [x] In the `list` command, check that `Microsoft.WinGet.Client` module is importable; write a diagnostic to stderr and exit `1` if absent

## list command

- [x] Implement `list` using `Get-WinGetPackage` and output each package as `<Id>\t<Version>` (omit trailing tab when version is empty)

## prepare command

- [x] Implement `prepare` by running `winget source update`; propagate non-zero exit as exit code `2`

## finalize command

- [x] Implement `finalize` as an immediate `exit 0`

## update-list command

- [x] Implement `update-list` returning exit code `1` to trigger sm-agent fallback

## install command

- [x] Implement `install <id>` (no `--file`) using `Install-WinGetPackage -Id <id> -Scope Machine -Mode Silent`
- [x] Add `--module-version` support: pass version to `Install-WinGetPackage -Version` when provided
- [x] Implement `install <id> --file <path>`: validate file exists (exit `2` if not), then run `winget install --silent --scope machine <path>`
- [x] Return exit `0` when the package is already installed at the requested version (idempotent install)
- [x] Map winget "not found" errors to exit code `2`
- [x] Map network/download errors to exit code `3`

## remove command

- [x] Implement `remove <id>` using `Uninstall-WinGetPackage -Id <id> -Mode Silent`
- [x] Add `--module-version` support: pass version to `Uninstall-WinGetPackage -Version` when provided
- [x] Return exit `0` when the package is not installed (idempotent remove)
- [x] Map uninstall failures (package blocks removal, etc.) to exit code `2`

## Packaging

- [x] Add `winget.ps1` to the Windows package/installer so it is placed in `C:\ProgramData\tedge\sm-plugins\` on install

## Tests

- [x] Add Robot Framework tests for `winget list` (happy path and missing-module error)
- [x] Add Robot Framework tests for `winget install` from a winget source
- [x] Add Robot Framework tests for `winget install` from a local file path
- [x] Add Robot Framework tests for `winget remove` (installed and not-installed cases)
- [x] Add Robot Framework tests for `winget prepare` and `winget finalize`
- [x] Add Robot Framework test verifying `winget update-list` returns exit code `1`
