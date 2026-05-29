*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO
Library             Collections

Suite Setup         Custom Setup
Suite Teardown      Get Suite Logs

# These tests run on a real Windows device/VM with winget and
# Microsoft.WinGet.Client installed. Tag them so they can be selected
# or excluded in CI.
Test Tags           theme:software    theme:plugins    platform:windows


*** Variables ***
# Absolute path to the installed plugin on the device under test.
${PLUGIN}           powershell.exe -NoProfile -ExecutionPolicy Bypass -File "C:\\ProgramData\\tedge\\sm-plugins\\winget.ps1"

# A stable, small winget package used for install/remove tests.
# Microsoft.PowerShell is already a known package and is tiny enough for CI.
${TEST_PKG_ID}      Microsoft.PowerShell
${TEST_PKG_VER}     7.4.6.0


*** Test Cases ***
# ---------------------------------------------------------------------------
# list
# ---------------------------------------------------------------------------

list outputs tab-separated package lines
    ${output}=    Execute Command    ${PLUGIN} list    exp_exit_code=0
    # Each line must be either "Id\tVersion" or just "Id" — no blank lines in the middle.
    FOR    ${line}    IN    @{output.splitlines()}
        ${parts}=    Split String    ${line}    \t
        ${count}=    Get Length    ${parts}
        Should Be True    ${count} >= 1 and ${count} <= 2
        ...    msg=Unexpected format on line: '${line}'
    END

list exits 1 when Microsoft.WinGet.Client module is absent
    # Temporarily rename the module to simulate it being missing.
    ${mod_path}=    Execute Command
    ...    powershell.exe -NoProfile -Command "(Get-Module -ListAvailable Microsoft.WinGet.Client | Select-Object -First 1).ModuleBase"
    ...    strip=${True}
    Skip If    '${mod_path}' == ''    Microsoft.WinGet.Client not installed on this device
    Execute Command    Rename-Item "${mod_path}" "${mod_path}.bak"    shell=pwsh
    ${stderr}=    Execute Command
    ...    ${PLUGIN} list
    ...    exp_exit_code=1    stdout=${False}    stderr=${True}
    Should Contain    ${stderr}    Microsoft.WinGet.Client
    [Teardown]    Execute Command
    ...    Rename-Item "${mod_path}.bak" "${mod_path}"    shell=pwsh

# ---------------------------------------------------------------------------
# prepare
# ---------------------------------------------------------------------------

prepare refreshes winget sources and exits 0
    Execute Command    ${PLUGIN} prepare    exp_exit_code=0

# ---------------------------------------------------------------------------
# finalize
# ---------------------------------------------------------------------------

finalize exits 0 without any action
    Execute Command    ${PLUGIN} finalize    exp_exit_code=0

# ---------------------------------------------------------------------------
# update-list
# ---------------------------------------------------------------------------

update-list exits 1 to signal sm-agent fallback
    Execute Command    ${PLUGIN} update-list    exp_exit_code=1

# ---------------------------------------------------------------------------
# install from winget source
# ---------------------------------------------------------------------------

install package by ID exits 0
    Execute Command
    ...    ${PLUGIN} install ${TEST_PKG_ID}
    ...    exp_exit_code=0

install package with explicit version exits 0
    Execute Command
    ...    ${PLUGIN} install ${TEST_PKG_ID} --module-version ${TEST_PKG_VER}
    ...    exp_exit_code=0

install already-installed package is idempotent
    Execute Command
    ...    ${PLUGIN} install ${TEST_PKG_ID}
    ...    exp_exit_code=0
    Execute Command
    ...    ${PLUGIN} install ${TEST_PKG_ID}
    ...    exp_exit_code=0

install unknown package ID exits 2
    Execute Command
    ...    ${PLUGIN} install com.example.ThisPackageDoesNotExist12345
    ...    exp_exit_code=2

# ---------------------------------------------------------------------------
# install from local file
# ---------------------------------------------------------------------------

install from local exe file exits 0
    [Documentation]    tedge-agent downloads the binary and passes the local path via --file.
    ...    Here we download a tiny portable exe as a stand-in for a real installer.
    # Use an already-downloaded file if available; otherwise skip.
    ${file}=    Set Variable    C:\\Windows\\Temp\\tedge-test-installer.exe
    Skip If    not Execute Command    Test-Path "${file}"    shell=pwsh    exp_exit_code=0
    Execute Command
    ...    ${PLUGIN} install TestApp --file "${file}"
    ...    exp_exit_code=0

install exits 2 when --file path does not exist
    ${stderr}=    Execute Command
    ...    ${PLUGIN} install TestApp --file "C:\\Windows\\Temp\\nonexistent-file-xyz.exe"
    ...    exp_exit_code=2    stdout=${False}    stderr=${True}
    Should Contain    ${stderr}    not found

# ---------------------------------------------------------------------------
# remove
# ---------------------------------------------------------------------------

remove installed package exits 0
    # Ensure the package is installed first.
    Execute Command
    ...    ${PLUGIN} install ${TEST_PKG_ID}
    ...    exp_exit_code=0
    Execute Command
    ...    ${PLUGIN} remove ${TEST_PKG_ID}
    ...    exp_exit_code=0

remove package that is not installed exits 0
    # First ensure it is not installed; ignore failure.
    Execute Command    ${PLUGIN} remove ${TEST_PKG_ID}    exp_exit_code=ANY
    # Now removing again must be a no-op.
    Execute Command
    ...    ${PLUGIN} remove ${TEST_PKG_ID}
    ...    exp_exit_code=0

remove with version exits 0
    Execute Command
    ...    ${PLUGIN} install ${TEST_PKG_ID} --module-version ${TEST_PKG_VER}
    ...    exp_exit_code=0
    Execute Command
    ...    ${PLUGIN} remove ${TEST_PKG_ID} --module-version ${TEST_PKG_VER}
    ...    exp_exit_code=0

# ---------------------------------------------------------------------------
# argument errors
# ---------------------------------------------------------------------------

unknown command exits 1
    ${stderr}=    Execute Command
    ...    ${PLUGIN} frobnicate
    ...    exp_exit_code=1    stdout=${False}    stderr=${True}
    Should Contain    ${stderr}    Unknown command


*** Keywords ***
Custom Setup
    Setup
    # Copy the plugin into the sm-plugins directory on the device under test.
    Transfer To Device    ${CURDIR}/../../../../configuration/contrib/sm-plugins/winget.ps1
    ...    C:\\ProgramData\\tedge\\sm-plugins\\winget.ps1
    # Ensure Microsoft.WinGet.Client is available (best-effort; individual
    # tests Skip when absent rather than failing the whole suite).
    Execute Command
    ...    powershell.exe -NoProfile -Command "if (-not (Get-Module -ListAvailable Microsoft.WinGet.Client)) { Install-Module Microsoft.WinGet.Client -Force -AllowClobber }"
    ...    exp_exit_code=0
