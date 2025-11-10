---
title: HSM Integration
tags: [Operate, Security, Cloud]
description: Using %%te%% with a Hardware Security Module
---

This guide aims to explain how to use %%te%% with a Hardware Security Module (HSM). %%te%% supports any HSM that provides the PKCS#11 interface which covers common HSMs like TPM2.0 and USB based tokens.

## Initializing the device

### TPM 2.0

1. Add the %%te%% user to the tpm2 group

    ```sh
    sudo usermod -a -G tss tedge
    ```

1. Create a new slot, and set the HSM's pin and so-pin.

    ```sh
    sudo softhsm2-util --init-token --free --label tedge --pin=123456 --so-pin=12345678
    ```

1. 
    ```sh
    usermod -a -G tss tedge
    sudo mkdir -p "/etc/tedge/hsm"
    sudo chown -R tedge:tedge "/etc/tedge/hsm"
    
    echo "TPM2_PKCS11_STORE=/etc/tedge/hsm" | sudo tee -a /etc/tedge/plugins/tedge-p11-server.conf
    ```

1. Configure the path to the TPM 2.0's pkcs11 module (aka .so file)

    ```sh
    PKCS11_MODULE=$(sudo find /usr/lib -name libsofthsm2.so | head -n1)
    sudo tedge config set device.cryptoki.module_path "$PKCS11_MODULE"
    ```

    :::note
    This path may be different on your device. Please check your TPM 2.0 installation to see where the libtpm2_pkcs11.so file is located.
    :::


### SoftHSM2

1. Add the %%te%% user to the softhsm group

    ```sh
    sudo usermod -a -G softhsm tedge
    ```

1. Create a new slot, and set the HSM's pin and so-pin.

    ```sh
    sudo softhsm2-util --init-token --free --label tedge --pin=123456 --so-pin=12345678
    ```

1. Configure the path to the SoftHSM2's pkcs11 module (.so) file

    ```sh
    sudo tedge config set device.cryptoki.module_path /usr/lib/softhsm/libsofthsm2.so
    ```

    :::note
    This path may be different on your device. Please check your SoftHSM2 installation to see where the libsofthsm2.so file is located.
    :::
