*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:troubleshooting    theme:plugins


*** Test Cases ***
Supports the c8y_Command operation out of the box
    Cumulocity.Should Contain Supported Operations    c8y_Command
    File Should Exist    /etc/tedge/operations/shell_execute.toml
    File Should Exist    /etc/tedge/operations/c8y/c8y_Command.template
    Symlink Should Exist    /etc/tedge/operations/c8y/c8y_Command

Successful shell command with output
    ${operation}=    Cumulocity.Execute Shell Command    echo helloworld
    Operation Should Be SUCCESSFUL    ${operation}
    Should Be Equal    ${operation.to_json()["c8y_Command"]["result"]}    helloworld\n

Check Successful shell command with literal double quotes output
    ${operation}=    Cumulocity.Execute Shell Command    echo \\"helloworld\\"
    Operation Should Be SUCCESSFUL    ${operation}
    Should Be Equal    ${operation.to_json()["c8y_Command"]["result"]}    "helloworld"\n

Execute multiline shell command
    ${operation}=    Cumulocity.Execute Shell Command    echo "hello"${\n}echo "world"
    Operation Should Be SUCCESSFUL    ${operation}
    Should Be Equal    ${operation.to_json()["c8y_Command"]["result"]}    hello\nworld\n

Truncates an output larger than the configured limit
    Execute Command    tedge config set shell.max_output_size 64
    ${operation}=    Cumulocity.Execute Shell Command    text=yes hello | head -n 1000
    Operation Should Be SUCCESSFUL    ${operation}
    Should Contain
    ...    ${operation.to_json()["c8y_Command"]["result"]}
    ...    <the output has been truncated after 64 bytes>
    [Teardown]    Run Keywords
    ...    Execute Command    tedge config unset shell.max_output_size
    ...    AND    Get Logs

Reports a large non ASCII output without crashing the mapper
    [Documentation]    The result is trimmed by the mapper to fit the Cumulocity payload limit,
    ...    which must not cut a multi byte character in half
    Execute Command    tedge config set shell.max_output_size 65536
    ${operation}=    Cumulocity.Execute Shell Command    text=yes 'éé' | head -n 100000
    Operation Should Be SUCCESSFUL    ${operation}
    Service Health Status Should Be Up    tedge-mapper-c8y
    [Teardown]    Run Keywords
    ...    Execute Command    tedge config unset shell.max_output_size
    ...    AND    Get Logs

Fails when the command returns a non-zero exit code
    ${operation}=    Cumulocity.Execute Shell Command    text=echo oops >&2; exit 1
    Operation Should Be FAILED    ${operation}    failure_reason=.*Command returned exit code 1: oops.*

Shell command succeeds if output is too large
    [Documentation]    Output should be trimmed by c8y mapper.
    ${operation}=    Cumulocity.Execute Shell Command    yes 'hello"' | head -n 100000
    Operation Should Be SUCCESSFUL    ${operation}
    ${result}=    Set Variable    ${operation.to_json()["c8y_Command"]["result"]}
    Should End With    ${result}    ...<trimmed>

Commands fail if the tmp.dir does not exist and include the path in the failure reason
    [Tags]    \#3796
    Execute Command    cmd=tedge config set tmp.path /dummy
    ${operation}=    Cumulocity.Execute Shell Command    echo helloworld
    Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=.*cannot create a temporary output file in the tmp dir '/dummy': No such file or directory.*
    [Teardown]    Run Keywords
    ...    Execute Command    cmd=tedge config unset tmp.path
    ...    AND    Get Logs

Supports disabling the Cumulocity c8y_Command operation
    Execute Command    tedge config set c8y.enable.shell_execute false
    Execute Command    rm -f /etc/tedge/operations/c8y/c8y_Command

    Restart Service    tedge-mapper-c8y
    Service Health Status Should Be Up    tedge-mapper-c8y
    Should Not Contain Supported Operations    c8y_Command

    ${operation}=    Cumulocity.Execute Shell Command    text=echo helloworld
    Operation Should Be PENDING    ${operation}    wait=15

    # Cleanup the pending operation, as it would otherwise pollute the test report output
    Execute Command    tedge mqtt pub c8y/s/us '505,${operation.to_json()["id"]},Cancelled operation'
    Operation Should Be FAILED    ${operation}
    [Teardown]    Run Keywords
    ...    Execute Command    tedge config unset c8y.enable.shell_execute
    ...    AND    Restart Service    tedge-mapper-c8y
    ...    AND    Get Logs

Restores the workflow definition when it has been removed
    Execute Command    rm -f /etc/tedge/operations/shell_execute.toml
    Restart Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent
    File Should Exist    /etc/tedge/operations/shell_execute.toml

Terminates the command and the processes it started when the timeout expires
    Execute Command    sed -i 's/^timeout_second = .*/timeout_second = 5/' /etc/tedge/operations/shell_execute.toml
    Restart Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent

    ${operation}=    Cumulocity.Execute Shell Command    text=sleep 61 & sleep 62
    Operation Should Be FAILED    ${operation}    failure_reason=.*Command timed out.*
    Process Should Not Be Running    sleep 61
    Process Should Not Be Running    sleep 62
    [Teardown]    Run Keywords
    ...    Execute Command    rm -f /etc/tedge/operations/shell_execute.toml
    ...    AND    Restart Service    tedge-agent
    ...    AND    Service Health Status Should Be Up    tedge-agent
    ...    AND    Get Logs

Supports the shell_execute command without any cloud
    Execute Command
    ...    tedge mqtt pub --retain te/device/main///cmd/shell_execute/local-1234 '{"status":"init","command":"echo hello"}'
    ${messages}=    Should Have MQTT Messages
    ...    te/device/main///cmd/shell_execute/local-1234
    ...    message_contains="status":"successful"
    ...    minimum=1
    Should Contain    ${messages[0]}    hello
    [Teardown]    Run Keywords
    ...    Execute Command    tedge mqtt pub --retain te/device/main///cmd/shell_execute/local-1234 ''
    ...    AND    Get Logs

Executes the command using the shell configured in tedge.toml
    Execute Command    printf '#!/bin/sh\\necho custom shell\\n' >/usr/bin/tedge-test-shell
    Execute Command    chmod a+x /usr/bin/tedge-test-shell
    Execute Command    tedge config set shell.path /usr/bin/tedge-test-shell

    ${operation}=    Cumulocity.Execute Shell Command    text=echo helloworld
    Operation Should Be SUCCESSFUL    ${operation}
    Should Be Equal    ${operation.to_json()["c8y_Command"]["result"]}    custom shell\n
    [Teardown]    Run Keywords
    ...    Execute Command    cmd=tedge config unset shell.path
    ...    AND    Get Logs

Supports disabling the shell_execute command on the device
    Execute Command    touch /etc/tedge/operations/shell_execute.toml.disabled
    Execute Command    rm -f /etc/tedge/operations/shell_execute.toml
    Restart Service    tedge-agent
    Service Health Status Should Be Up    tedge-agent
    File Should Not Exist    /etc/tedge/operations/shell_execute.toml
    [Teardown]    Run Keywords
    ...    Execute Command    rm -f /etc/tedge/operations/shell_execute.toml.disabled
    ...    AND    Restart Service    tedge-agent
    ...    AND    Get Logs


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

Process Should Not Be Running
    [Documentation]    Check no process runs with exactly the given command line,
    ...    using /proc rather than pgrep which is not installed on every image
    [Arguments]    ${command_line}
    Execute Command
    ...    for f in /proc/[0-9]*/cmdline; do tr '\\0' ' ' < "$f" 2>/dev/null; echo; done | grep -c '^${command_line} $'
    ...    exp_exit_code=1
