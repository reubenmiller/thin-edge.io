---
title: Using thin-edge.io with a HSM 2
tags: [Reference, Security]
description: Using a HSM with %%te%%.
---

import UserContext from '@site/src/components/UserContext';
import UserContextForm from '@site/src/components/UserContextForm';

:::tip
#### User Context {#user-context}

You can customize the documentation and commands shown on this page by providing
relevant settings which will be reflected in the instructions. It makes it even
easier to explore and use %%te%%.

<UserContextForm settings="C8Y_PROFILE_NAME,C8Y_PROFILE_URL,C8Y_URL,DEVICE_ID" />

The user context will be persisted in your web browser's local storage.
:::

This section describes how to use %%te%% with a Hardware Security Module (HSM) by way of an PKCS#11 interface.

## Overview

%%te%% supports HSM via the PKCS#11 interface (a.k.a. cryptoki) to allow the usage of private keys generated and stored by HSMs to be used when establishing the connection to the cloud.

All HSMs which support the [PKCS#11 interface](https://docs.oasis-open.org/pkcs11/pkcs11-base/v2.40/os/pkcs11-base-v2.40-os.html) can be supported, these included:
* Nitrokey HSM 2
* Yubikey 5C (with PIV)
* TPM 2.0
* Arm TrustZone (via OP-TEE)

## Pre-requisites

* the mapper must be configured to use the built-in bridge (e.g. `mqtt.bridge.built_in` should be set to `true`)
* the `tedge` user MUST have access to the HSM (sometimes this involves adding the `tedge` user to a specific group on the device)

## Configuration

### Step 1: Install tedge-p11-server

The **tedge-p11-server** is an optional component which needs to be installed which will provides an interface for %%te%% to use to access a private key stored in a HSM via the PKCS#11 interface.

The package can be installed from the public %%te%% Cloudsmith.io repository using:

```sh
sudo apt-get install tedge-p11-server
```

The package includes a SystemD socket definition, called `tedge-p11-server.socket`. When %%te%% tries to use the unix socket, the `tedge-p11-server.service` will be automatically started by SystemD (this is called [socket activation](https://0pointer.de/blog/projects/socket-activation.html)). You can list the details of the socket by using the following command:

```sh
sudo systemctl status tedge-p11-server.socket
```

You will need to configure the tedge-p11-server with your desired PKCS11 module:

```plain title="file: /etc/tedge/plugins/tedge-p11-server.conf"
TEDGE_DEVICE_CRYPTOKI_MODULE_PATH=/usr/lib/aarch64-linux-gnu/pkcs11/libtpm2_pkcs11.so
TEDGE_DEVICE_CRYPTOKI_PIN=123456
TPM2_PKCS11_STORE="/etc/tedge/device-certs"
```

### Step 2: Set the cryptoki mode

Activate the `cryptoki` mode so that %%te%% knows that it should be using the PKCS#11 (cryptoki) interface to access the device certificate's private key instead of from the file system.

```sh
tedge config set device.cryptoki.mode socket
```

### Step 3: Enable the build-in bridge

Configure %%te%% to use the built-in bridge (instead of the mosquitto bridge):

```sh
tedge config set mqtt.bridge.built_in true
```

### Step 4: Create the private key

Each HSM might have a slightly different way to configure it, so if you having any problems with initializing your HSM, then please consult the manufacturers documentation.

### Step 5: Get a device certificate

A public certificate can either issued by either creating a self-signed certificate and then uploading it to Cumulocity, or by creating a Certificate Signing Request, and sending the request to a Certificate Authority which will then return a valid certificate.

### Step 6: Connect to Cumulocity

Once the HSM have been configured, and you have a device certificate, then you can connect to the cloud:

```sh
tedge connect c8y
```

<UserContext title="Output">

```text
# tedge connect c8y
Connecting to Cumulocity with config:
	device id: $DEVICE_ID
	cloud profile: <none>
	cloud host: $C8Y_URL:8883
	auth type: Certificate
	certificate file: /etc/tedge/device-certs/tedge-certificate.pem
	cryptoki: true
	bridge: built-in
	service manager: systemd
	mosquitto version: 2.0.11
Creating device in Cumulocity cloud... ✓
Restarting mosquitto... ✓
Waiting for mosquitto to be listening for connections... ✓
Enabling tedge-mapper-c8y... ✓
Verifying device is connected to cloud... ✓
Enabling tedge-agent... ✓
```

</UserContext>


## Useful commands

### List tokens on the device

```sh
p11tool --list-tokens
```

## Examples

### Start tedge-p11-server

```sh
tedge config set device.cryptoki.socket_path "/tmp/tedge-p11-server.sock"
tedge-p11-server --module-path "/opt/homebrew/lib/libykcs11.dylib" --socket-path "/tmp/tedge-p11-server.sock"
```

### Enroll the device

```sh
export DEVICE_ID=demo10005
ENROL_TOKEN=$(c8y deviceregistration register-ca --id "$DEVICE_ID" --one-time-password "$(c8y template execute --template '_.Password(31)')" --select password -o csv)
```

### Create new certificate

```sh
/Users/reubenmiller/dev/projects/thin-edge.io/code/tedge-rugpi-core/recipes/setup-pkcs11/files/init-pkcs11.sh --type yubikey --create --token "$ENROL_TOKEN"
```

### Renew certificate

```
/Users/reubenmiller/dev/projects/thin-edge.io/code/tedge-rugpi-core/recipes/setup-pkcs11/files/init-pkcs11.sh --type yubikey --renew
```


## HSM Specific instructions

### Yubikey

1. Make sure the Yubikey is connected

1. Install the [ykman CLI](https://docs.yubico.com/software/yubikey/tools/ykman/)

1. Create a private key along with the associated public key

    ```sh
    ykman piv keys generate --algorithm ECCP256 9a /etc/tedge/device-certs/public.key
    ```

1. Create the device certificate

    **Self-signed**

    If you're using a self-signed certificate, then you can create the certificate directly using:

    <UserContext>

    ```sh
    ykman piv certificates generate \
        --subject "CN=$DEVICE_ID,OU=Test Device,O=Thin Edge" \
        9a /etc/tedge/device-certs/public.key - > "$(tedge config get device.cert_path)"
    ```

    </UserContext>

    Then upload the self-signed certificate to Cumulocity

    ```sh
    tedge cert upload c8y
    ```

    **Certificate Signing Request**

    If you are using a certificate authority, then you will need to create a Certificate Signing Request, and then send it to get signed by the certificate-authority:

    <UserContext>

    ```sh
    ykman piv certificates request \
        --subject "CN=$DEVICE_ID,OU=Test Device,O=Thin Edge" \
        9a public.key - > "$(tedge config get device.csr_path)"
    ```

    </UserContext>

    <UserContext>

    ```sh
    tedge cert renew c8y --csr-path "$(tedge config get device.csr_path)"
    tedge reconnect c8y
    ```

    </UserContext>
