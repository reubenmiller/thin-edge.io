# PKI Device certificate interface example

There is a script called `tedge-cert.sh` which can be used to managed the certificate used by thin-edge.

The device.id (which will be used as the Common Name in the certificate) will be controlled by an environment variable (for now). So before using the `tedge-cert.sh` script, the environment variable needs to be set. Ideally this is done when creating the docker environment, or alternatively a dummy certificate can be created us


## Setting the device id (Common Name)

### Option 1: Set the environment variable in the docker container

You can set the `DEVICE_ID` environment variable when creating the docker container (with thin-edge.io installed inside it).

```sh
docker run -d --env DEVICE_ID=mydevice01 <myimage>
```

### Option 2: Create a dummy certificate

```
tedge cert create --device-id mydecice01
```


## Certificate Management Interface

The following commands are supported by the `tedge-cert.sh` script.

### Generate a CSR file

Generate a CSR file which can be sent to the PKI.

```sh
mkdir -p /tmp/
tedge-cert.sh csr --out-csr /tmp/mycsr.csr
```

### Update public certificate

```sh
tedge-cert.sh set --c8y-url example.cumulocity.io --certificate /tmp/certgen/device-cert.pem
```
