*** Settings ***
Library             DeviceLibrary
Library             Cumulocity

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           adapter:docker    theme:cryptoki


*** Test Cases ***
UX Testing
    # step 1 - HSM specific setup
    Execute Command    cmd=sudo usermod -a -G softhsm tedge
    Execute Command    cmd=softhsm2-util --init-token --free --label tedge --pin=123456 --so-pin=123456
    Execute Command    cmd=sudo tedge config set device.cryptoki.module_path /usr/lib/softhsm/libsofthsm2.so

    # step 2 - Configure HSM settings in thin-edge.io
    Execute Command    cmd=sudo tedge config set mqtt.bridge.built_in true
    Execute Command    cmd=sudo tedge config set device.cryptoki.mode socket
    Execute Command    cmd=sudo tedge config set device.cryptoki.pin 123456
    Execute Command    cmd=sudo systemctl restart tedge-p11-server
    Execute Command    cmd=sudo systemctl enable tedge-p11-server

    # step 3 - Create Key and Register with Cumulocity
    ${credentials}=    Cumulocity.Bulk Register Device With Cumulocity CA    external_id=${DEVICE_SN}

    Execute Command    cmd=sudo tedge config set c8y.url "${credentials.url}"
    Execute Command    cmd=sudo tedge cert create-key-pkcs11 "pkcs11:token=tedge"
    Execute Command
    ...    cmd=sudo tedge cert download c8y --device-id '${DEVICE_SN}' --one-time-password '${credentials.one_time_password}' --retry-every 5s --max-timeout=30s
    Execute Command    cmd=tedge reconnect c8y
    Log    Done

UX Testing with Helper
    Execute Command    cmd=sudo tedge-init-pkcs11.sh --type softhsm2 --pin 123456 --so-pin 12345678

    ${credentials}=    Cumulocity.Bulk Register Device With Cumulocity CA    external_id=${DEVICE_SN}

    Execute Command    cmd=sudo tedge config set c8y.url "${credentials.url}"
    Execute Command
    ...    cmd=sudo tedge cert download c8y --device-id '${DEVICE_SN}' --one-time-password '${credentials.one_time_password}' --retry-every 5s --max-timeout=30s
    Execute Command    cmd=tedge connect c8y
    Log    Done


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    register=${False}
    Set Suite Variable    ${DEVICE_SN}
