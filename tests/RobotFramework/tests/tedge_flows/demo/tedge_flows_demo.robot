*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Logs

Test Tags           theme:tedge_flows


*** Test Cases ***
Local Flows
    [Timeout]    60mins
    Execute Command    cmd=tedge mqtt pub te/device/main///e/foo '{"text": "test message '"$(date +%s)"'"}'
    Sleep    1s
    Execute Command    cmd=tedge mqtt pub te/device/main///e/foo '{"text": "test message '"$(date +%s)"'"}'
    Sleep    1s
    Execute Command    cmd=tedge mqtt pub te/device/main///e/foo '{"text": "test message '"$(date +%s)"'"}'
    Sleep    1s
    Execute Command    cmd=tedge mqtt pub te/device/main///e/foo '{"text": "test message '"$(date +%s)"'"}'
    Sleep    1s
    Execute Command    cmd=tedge mqtt pub te/device/main///e/foo '{"text": "test message '"$(date +%s)"'"}'
    Sleep    2s


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup    connect=${False}
    Set Suite Variable    $DEVICE_SN
    ${domain}=    Get Domain
    Execute Command    cmd=tedge config set c8y.mqtt_service.enabled true
    Execute Command    cmd=tedge config set c8y.mqtt ${domain}:9883
    Execute Command    cmd=curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/community/setup.deb.sh' | sudo -E bash
    Execute Command    cmd=apt-get install -y tedge-oscar tree tedge-command-plugin tedge-inventory-plugin tedge-parameter-plugin
    Execute Command
    ...    cmd=echo 'tedge ALL = (ALL) NOPASSWD:SETENV: /usr/bin/journalctl' | sudo tee -a /etc/sudoers.d/tedge
    Execute Command
    ...    cmd=echo 'tedge ALL = (ALL) NOPASSWD:SETENV: /usr/bin/systemctl' | sudo tee -a /etc/sudoers.d/tedge
    # Transfer To Device    src=${CURDIR}/sm_plugin    dst=/etc/tedge/sm-plugins/flow

    # install tools used in the demo
    Execute Command    cmd=wget -O /usr/bin/ttyd https://github.com/tsl0922/ttyd/releases/download/1.7.7/ttyd.aarch64 && chmod a+x /usr/bin/ttyd

    # TODO: Check if nologin should be removed if ttyd wants the user to login or not to prove credentials
    # rm /run/nologin

    Transfer To Device    src=${CURDIR}/ttyd-edit.service    dst=/usr/lib/systemd/system/ttyd-edit.service
    Transfer To Device    src=${CURDIR}/ttyd.service    dst=/usr/lib/systemd/system/ttyd.service
    Execute Command    cmd=systemctl enable --now ttyd-edit && systemctl enable --now ttyd

    Connect Mapper    mapper=c8y
    Device Should Exist    ${DEVICE_SN}    show_info=${False}

    ThinEdgeIO.Add Remote Access Passthrough Configuration    name=http:ttyd    port=7681
    ThinEdgeIO.Add Remote Access Passthrough Configuration    name=http:vim    port=7682

    Transfer To Device    src=${CURDIR}/tedge-config-context    dst=/etc/tedge/mappers/local/flows/
    Transfer To Device    src=${CURDIR}/tedge-events    dst=/etc/tedge/mappers/local/flows/

    # Manually loading flows requires a restart so that it sends the updated flows list
    # Restart Service    tedge-agent
    # Execute Command    cmd=chown -R tedge:tedge /etc/tedge/mappers/local/flows
    # FIXME: The tedge-mapper-local doesn't seem to create the flows directory which causes the inotify listener to fail
    # and any subsequent changes are not detected
    Enable Service    tedge-mapper-local
    Start Service    tedge-mapper-local

    Disconnect Then Connect Mapper    c8y
    Device Should Exist    ${DEVICE_SN}    show_info=${True}
