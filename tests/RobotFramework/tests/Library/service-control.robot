*** Settings ***
Resource    ../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO


*** Test Cases ***

Support starting and stopping services
    ${DEVICE_SN}                               Setup Device
    Device Should Exist                        ${DEVICE_SN}
    Process Should Be Running On Device        tedge_mapper c8y
    Stop Service                               tedge-mapper-c8y
    Process Should Not Be Running On Device    tedge_mapper c8y
    Start Service                              tedge-mapper-c8y
    Process Should Be Running On Device        tedge_mapper c8y
