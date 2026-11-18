*** Settings ***
Resource            windows.resource

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           platform:windows    theme:installation


*** Variables ***
@{EXPECTED_SIDS}    S-1-5-18    S-1-5-32-544


*** Test Cases ***
tedge is installed on the PATH
    ${path}=    Execute Command    (Get-Command tedge.exe -ErrorAction Stop).Source    strip=${True}
    Should Not Be Empty    ${path}
    Execute Command    tedge --version

First run creates the configuration directories
    Directory Should Exist    ${TEDGE_DIR}
    FOR    ${name}    IN    data    log    tmp    sm-plugins    config-plugins    log-plugins
        Directory Should Exist    ${TEDGE_DIR}\\${name}
    END

First run creates the tedge configuration file
    File Should Exist    ${TEDGE_DIR}\\tedge.toml
    ${built_in}=    Execute Command    tedge config get mqtt.bridge.built_in    strip=${True}
    Should Be Equal    ${built_in}    true

First run seeds the file log and configuration plugins
    File Should Exist    ${TEDGE_DIR}\\log-plugins\\file.cmd
    File Should Exist    ${TEDGE_DIR}\\config-plugins\\file.cmd

Default log types include the service logs
    ${types}=    Execute Command    Get-Content ${TEDGE_DIR}\\plugins\\tedge-log-plugin.toml -Raw
    Should Contain    ${types}    type = "tedge-agent"
    Should Contain    ${types}    type = "tedge-mapper"

Default configuration types include mosquitto.conf
    ${types}=    Execute Command    Get-Content ${TEDGE_DIR}\\plugins\\tedge-configuration-plugin.toml -Raw
    Should Contain    ${types}    type = "mosquitto"
    Should Contain    ${types}    mosquitto.conf

Configuration directory is restricted to SYSTEM and Administrators
    ${output}=    Execute Command
    ...    (Get-Acl -LiteralPath '${TEDGE_DIR}').Access | ForEach-Object { $_.IdentityReference.Translate([System.Security.Principal.SecurityIdentifier]).Value } | Sort-Object -Unique
    ...    strip=${True}
    @{sids}=    Split To Lines    ${output}
    Lists Should Be Equal    ${sids}    ${EXPECTED_SIDS}
    # Permissions inherited from C:\ProgramData (read access for all users) are removed
    ${protected}=    Execute Command
    ...    (Get-Acl -LiteralPath '${TEDGE_DIR}').AreAccessRulesProtected
    ...    strip=${True}
    Should Be Equal    ${protected}    True

Agent is installed as a service
    Service Should Be Running    tedge-agent
    Service Should Be Enabled    tedge-agent


*** Keywords ***
Custom Setup
    Windows Setup
    Ensure Service Is Running    tedge-agent
