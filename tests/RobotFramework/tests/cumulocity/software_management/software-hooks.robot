*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:software    theme:plugins
Test Setup       Custom Setup
Test Teardown    Custom Teardown

*** Test Cases ***
Softare update operation with user confirmation
    ${OPERATION}=    Cumulocity.Install Software    {"softwareType":"apt","name":"vim-tiny","version":"latest"}
    Operation Should Be SUCCESSFUL    ${OPERATION}

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=                            Setup
    Device Should Exist                      ${DEVICE_SN}
    Set Test Variable    $DEVICE_SN
    Transfer To Device    ${CURDIR}/software_update.toml    /etc/tedge/operations/
    Transfer To Device    ${CURDIR}/user_confirm.py    /usr/bin/
    Execute Command    chmod 644 /lib/systemd/system/confirm.service && systemctl daemon-reload
    Execute Command    apt-get install -y python3-minimal
    Restart Service    tedge-agent

Custom Teardown
    Get Logs
