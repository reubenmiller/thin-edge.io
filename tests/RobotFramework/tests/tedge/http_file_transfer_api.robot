*** Comments ***
# Command to execute:    robot -d \results --timestampoutputs --log http_file_transfer_api.html --report NONE -v BUILD:840 -v HOST:192.168.1.130 thin-edge.io/tests/RobotFramework/tedge/http_file_transfer_api.robot


*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown

Test Tags           theme:cli    theme:configuration    theme:childdevices


*** Variables ***
${DEVICE_SN}    ${EMPTY}    # Parent device serial number
${DEVICE_IP}    ${EMPTY}    # Parent device host name which is reachable
${PORT}         8000


*** Test Cases ***
Get Put Delete
    Setup    skip_bootstrap=True    # Setup child device

    Execute Command    curl -X PUT -d "test of put" http://${DEVICE_IP}:${PORT}/tedge/file-transfer/file_a
    ${get}=    Execute Command    curl --silent http://${DEVICE_IP}:${PORT}/tedge/file-transfer/file_a
    Should Be Equal    ${get}    test of put
    Execute Command    curl -X DELETE http://${DEVICE_IP}:${PORT}/tedge/file-transfer/file_a

File transfer using tedge cli
    Setup    skip_bootstrap=False

    Execute Command    tedge http put /tedge/file-transfer/file_b "content to be transferred"
    ${content}=    Execute Command    tedge http get /tedge/file-transfer/file_b
    Should Be Equal    ${content}    content to be transferred
    Execute Command    tedge http delete /tedge/file-transfer/file_b
    Execute Command    tedge http get /tedge/file-transfer/file_b    exp_exit_code=1


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=False
    Set Suite Variable    $DEVICE_SN    ${DEVICE_SN}

    ${DEVICE_IP}=    Get IP Address
    Set Suite Variable    ${DEVICE_IP}

    Execute Command    sudo tedge config set mqtt.external.bind.address ${DEVICE_IP}
    ${bind}=    Execute Command    tedge config get mqtt.external.bind.address    strip=True
    Should Be Equal    ${bind}    ${DEVICE_IP}
    Execute Command    sudo -u tedge mkdir -p /var/tedge
    Restart Service    tedge-agent

Custom Teardown
    Set Device Context    ${DEVICE_SN}
    Execute Command    sudo rm -rf /var/tedge/file-transfer
    Get Suite Logs
