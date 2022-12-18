*** Settings ***
Resource    ../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO


*** Test Cases ***

Support starting and stopping services
    ${DEVICE_SN}                     Setup
    Device Should Exist              ${DEVICE_SN}
    Process Should Be Running        tedge_mapper c8y
    Stop Service                     tedge-mapper-c8y
    Process Should Not Be Running    tedge_mapper c8y
    Start Service                    tedge-mapper-c8y
    Process Should Be Running        tedge_mapper c8y
