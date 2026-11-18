*** Settings ***
Resource            windows.resource

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           platform:windows    theme:capabilities


*** Test Cases ***
shell_execute is registered on Windows
    ${date_from}=    Get Unix Timestamp
    Restart Agent And Wait Until Up
    # The agent publishes its capabilities on startup
    Should Have MQTT Messages    te/device/main///cmd/shell_execute    date_from=${date_from}

shell_execute runs a PowerShell command
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/shell_execute/robot-win-1 '{"status":"init","command":"Write-Output (''hello '' + $PSVersionTable.PSEdition)"}'
    ${messages}=    Should Have MQTT Messages
    ...    te/device/main///cmd/shell_execute/robot-win-1
    ...    message_pattern="status":\\s*"successful"
    ...    minimum=1
    Should Contain    ${messages}[-1]    hello Desktop
    [Teardown]    Run Keywords
    ...    Execute Command    tedge mqtt pub --retain te/device/main///cmd/shell_execute/robot-win-1 ''
    ...    AND    Get Logs

shell_execute reports a failing command with its last output line
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/shell_execute/robot-win-2 '{"status":"init","command":"[Console]::Error.WriteLine(''oops''); exit 3"}'
    ${messages}=    Should Have MQTT Messages
    ...    te/device/main///cmd/shell_execute/robot-win-2
    ...    message_pattern="status":\\s*"failed"
    ...    minimum=1
    Should Contain    ${messages}[-1]    Command returned exit code 3: oops
    [Teardown]    Run Keywords
    ...    Execute Command    tedge mqtt pub --retain te/device/main///cmd/shell_execute/robot-win-2 ''
    ...    AND    Get Logs

shell_execute terminates a command and its child processes on timeout
    Execute Command    tedge config set shell.timeout 5s
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/shell_execute/robot-win-3 '{"status":"init","command":"Write-Output started; Start-Process -WindowStyle Hidden ping.exe -ArgumentList ''-n 1234 127.0.0.1''; Start-Sleep 1235"}'
    ${messages}=    Should Have MQTT Messages
    ...    te/device/main///cmd/shell_execute/robot-win-3
    ...    message_pattern="status":\\s*"failed"
    ...    minimum=1
    ...    timeout=60
    Should Contain    ${messages}[-1]    Command timed out after 5s: started
    ${remaining}=    Execute Command
    ...    @(Get-CimInstance Win32_Process -Filter "Name = 'ping.exe'" | Where-Object CommandLine -like '*1234*').Count
    ...    strip=${True}
    Should Be Equal    ${remaining}    0
    [Teardown]    Run Keywords
    ...    Execute Command    tedge config unset shell.timeout
    ...    AND    Execute Command    tedge mqtt pub --retain te/device/main///cmd/shell_execute/robot-win-3 ''
    ...    AND    Get Logs


*** Keywords ***
Custom Setup
    Windows Setup
    Ensure Service Is Running    tedge-agent
