*** Settings ***
Resource            windows.resource

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           platform:windows    theme:services


*** Variables ***
${HEALTH_TOPIC}     te/device/main/service/tedge-agent/status/health


*** Test Cases ***
Start tedge-agent service publishes health up
    Stop Service    tedge-agent
    Service Should Be Stopped    tedge-agent
    ${date_from}=    Get Unix Timestamp
    Start Service    tedge-agent
    ${pid}=    Service Should Be Running    tedge-agent
    Should Be True    ${pid} > 0
    Should Have MQTT Messages
    ...    ${HEALTH_TOPIC}
    ...    date_from=${date_from}
    ...    message_pattern="status":\\s*"up"
    Service Health Status Should Be Up    tedge-agent
    [Teardown]    Run Keywords    Ensure Service Is Running    tedge-agent    AND    Get Logs

Stop tedge-agent service publishes health down
    Service Health Status Should Be Up    tedge-agent
    ${date_from}=    Get Unix Timestamp
    Stop Service    tedge-agent
    Service Should Be Stopped    tedge-agent
    Should Have MQTT Messages
    ...    ${HEALTH_TOPIC}
    ...    date_from=${date_from}
    ...    message_pattern="status":\\s*"down"
    Service Health Status Should Be Down    tedge-agent
    [Teardown]    Run Keywords    Ensure Service Is Running    tedge-agent    AND    Get Logs

Restart tedge-agent service
    ${pid_before}=    Service Should Be Running    tedge-agent
    ${date_from}=    Get Unix Timestamp
    Restart Service    tedge-agent
    ${pid_after}=    Service Should Be Running    tedge-agent
    Should Not Be Equal    ${pid_before}    ${pid_after}    msg=tedge-agent should have been restarted
    Should Have MQTT Messages
    ...    ${HEALTH_TOPIC}
    ...    date_from=${date_from}
    ...    message_pattern="status":\\s*"down"
    Should Have MQTT Messages
    ...    ${HEALTH_TOPIC}
    ...    date_from=${date_from}
    ...    message_pattern="status":\\s*"up"
    Service Health Status Should Be Up    tedge-agent


*** Keywords ***
Custom Setup
    Windows Setup
    Ensure Service Is Running    tedge-agent
