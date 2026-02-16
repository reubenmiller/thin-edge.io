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
    Execute Command    cmd=tedge mqtt pub te/device/main///e/foo '{"text": "test message '"$(date +%s)"'"}'
    Execute Command    cmd=tedge mqtt pub te/device/main///e/foo '{"text": "test message '"$(date +%s)"'"}'
    Execute Command    cmd=tedge mqtt pub te/device/main///e/foo '{"text": "test message '"$(date +%s)"'"}'
    Execute Command    cmd=tedge mqtt pub te/device/main///e/foo '{"text": "test message '"$(date +%s)"'"}'
    Sleep    2s


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup
    Set Suite Variable    $DEVICE_SN
    ${domain}=    Get Domain
    Execute Command    cmd=tedge config set c8y.mqtt_service.enabled true
    Execute Command    cmd=tedge config set c8y.mqtt ${domain}:9883
    Execute Command    cmd=curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/community/setup.deb.sh' | sudo -E bash
    Execute Command    cmd=apt-get install -y tedge-oscar tree tedge-command-plugin
    Execute Command
    ...    cmd=echo 'tedge ALL = (ALL) NOPASSWD:SETENV: /usr/bin/journalctl' | sudo tee -a /etc/sudoers.d/tedge
    Execute Command
    ...    cmd=echo 'tedge ALL = (ALL) NOPASSWD:SETENV: /usr/bin/systemctl' | sudo tee -a /etc/sudoers.d/tedge
    Transfer To Device    src=${CURDIR}/sm_plugin    dst=/etc/tedge/sm-plugins/flow

    # install tools used in the demo
    Execute Command    cmd=wget -O /usr/bin/ttyd https://github.com/tsl0922/ttyd/releases/download/1.7.7/ttyd.aarch64 && chmod a+x /usr/bin/ttyd

    # TODO: Check if nologin should be removed if ttyd wants the user to login or not to prove credentials
    # rm /run/nologin

    Transfer To Device    src=${CURDIR}/ttyd-edit.service    dst=/usr/lib/systemd/system/ttyd-edit.service
    Transfer To Device    src=${CURDIR}/ttyd.service    dst=/usr/lib/systemd/system/ttyd.service
    Execute Command    cmd=systemctl enable --now ttyd-edit && systemctl enable --now ttyd

    Device Should Exist    ${DEVICE_SN}    show_info=${False}

    # Manually loading flows requires a restart so that it sends the updated flows list
    Restart Service    tedge-agent

    ThinEdgeIO.Add Remote Access Passthrough Configuration    name=http:ttyd    port=7681
    ThinEdgeIO.Add Remote Access Passthrough Configuration    name=http:vim    port=7682

    # TODO: Potential race condition which could result in files not being loaded correctly
    # on startup - but not getting the notifications for them?
    # Install some local flows => tedge-config provides state to the other
    Transfer To Device    src=${CURDIR}/tedge-config-context    dst=/etc/tedge/mappers/flows/flows/
    Transfer To Device    src=${CURDIR}/tedge-events    dst=/etc/tedge/mappers/flows/flows/

    # FIXME: Remove once https://github.com/thin-edge/thin-edge.io/issues/3979 is resolved
    Restart Service    tedge-flows

    Disconnect Then Connect Mapper    c8y
    Device Should Exist    ${DEVICE_SN}    show_info=${True}
