*** Settings ***
Resource    ../../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:troubleshooting    theme:plugins
Test Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***

Remote access should be independent of tedge-mapper-c8y
    Execute Remote Access Command    sudo systemctl restart tedge-agent    device=${DEVICE_SN}    user=iotadmin
    Log    hello

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

    Transfer To Device    ${CURDIR}/c8y-remote-access-plugin@.service    /lib/systemd/system/
    Transfer To Device    ${CURDIR}/c8y-remote-access-plugin.socket    /lib/systemd/system/
    Transfer To Device    ${CURDIR}/c8y_RemoteAccessConnect    /etc/tedge/operations/c8y/
    Transfer To Device    ${CURDIR}/publish_to_socket.sh    /usr/bin/
    Execute Command    systemctl daemon-reload && systemctl start c8y-remote-access-plugin.socket && systemctl enable c8y-remote-access-plugin.socket
    Restart Service    tedge-mapper-c8y
    Execute Command    sudo useradd -ms /bin/bash "iotadmin" && echo "iotadmin:iotadmin" | sudo chpasswd && sudo adduser "iotadmin" sudo
    Execute Command    systemctl start sshd
