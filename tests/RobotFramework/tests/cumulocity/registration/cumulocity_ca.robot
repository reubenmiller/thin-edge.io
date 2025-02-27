*** Settings ***
Resource        ../../../../resources/common.resource
Library         Cumulocity
Library         ThinEdgeIO
Library         String

Test Setup      Custom Setup

Test Tags       theme:c8y    test:on_demand


*** Test Cases ***
Register Device Using Cumulocity CA
    ${credentials}=    Bulk Register Device With Cumulocity CA    ${DEVICE_SN}
    ${DOMAIN}=    Get Cumulocity Domain
    Execute Command    tedge config set c8y.url "${DOMAIN}"
    Execute Command    tedge cert download c8y --device-id "${DEVICE_SN}" --token "${credentials.password}"
    Execute Command    tedge connect c8y


*** Keywords ***
Get Cumulocity Domain
    ${DOMAIN}=    Replace String Using Regexp    ${C8Y_CONFIG.host}    ^.*://    ${EMPTY}
    ${DOMAIN}=    Strip String    ${DOMAIN}    characters=/
    RETURN    ${DOMAIN}

Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --no-bootstrap --no-connect || true

    Set Test Variable    $DEVICE_SN
