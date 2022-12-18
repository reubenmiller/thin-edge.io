*** Settings ***
Resource    ../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO    adapter=docker


*** Test Cases ***

Supports a reconnect
    ${DEVICE_SN}                         Setup Device
    Device Should Exist                  ${DEVICE_SN}
    Tedge Disconnect Then Connect        mapper=c8y    sleep=5

Supports disconnect then connect
    ${DEVICE_SN}                         Setup Device
    Device Should Exist                  ${DEVICE_SN}
    Tedge Disonnect
    Tedge Connect

Update unknown setting
    ${DEVICE_SN}                         Setup Device
    Device Should Exist                  ${DEVICE_SN}
    Execute Command On Device            tedge config set unknown.value 1    exp_exit_code=2

Update known setting
    ${DEVICE_SN}                         Setup Device
    Set Tedge Configuration Using CLI    device.type        mycustomtype
    ${OUTPUT}=    Execute Command On Device            tedge config get device.type
    Should Match    ${OUTPUT}            mycustomtype\n
