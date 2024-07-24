#!/usr/bin/env python3
import json
import logging
import subprocess
import sys
import time
from typing import Dict, Union, Any

# thin-edge.io device identifier
TOPIC_PREFIX = "te"
TOPIC_ID = "device/main//"

# Exit codes
EXIT_OK = 0
EXIT_NO_CONFIRM = 100
EXIT_ERROR = 1

# Set sensible logging defaults
log = logging.getLogger()
log.setLevel(logging.INFO)
handler = logging.StreamHandler()
handler.setLevel(logging.INFO)
formatter = logging.Formatter("%(asctime)s - %(name)s - %(levelname)s - %(message)s")
handler.setFormatter(formatter)
log.addHandler(handler)


def publish_message(topic: str, payload: Union[Dict[str, Any], str], qos: int = 1):
    """Publish MQTT message using the tedge cli, as this is the most
    flexible as the tedge cli auto detects whether local certs are being used
    to authenticate against the local MQTT broker or not.
    """
    if not isinstance(payload, str):
        payload_json = json.dumps(payload)
    else:
        payload_json = payload

    proc = subprocess.Popen([
        "tedge",
        "mqtt",
        "pub",
        f"--qos={qos}", 
        topic,
        payload_json,
    ], stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, universal_newlines=True, text=True)
    code = proc.wait(30)
    if code != 0:
        log.warning("Failed to publish MQTT message. code=%d, stdout=%s", proc.returncode, proc.stdout)


def wait_for_user_confirmation(topic, payload, duration: float = 10):
    log.info("Received operation: topic=%s, payload=%s", topic, payload)
    
    is_confirmed = False
    expires_at = time.monotonic() + duration

    while time.monotonic() < expires_at:
        log.info("Checking for user confirmation",)
        publish_message(
            f"{TOPIC_ID}/{TOPIC_PREFIX}/e/confirm",
            {
                "text": "Waiting for user confirmation",
            },
        )
        #
        # Do user confirmation check
        #
        is_confirmed = False
        time.sleep(5)
    
    return is_confirmed


try:
    topic = ""
    payload = {}
    if len(sys.argv) < 3:
        raise ValueError("Missing required arguments. <TOPIC> <PAYLOAD>")

    topic = sys.argv[1]
    payload = json.loads(sys.argv[2])

    if not wait_for_user_confirmation(topic, payload, 60):
        # TODO: The executing transition is missing, so we have to do it manually here :(
        # Though it will not be required once operation can be updated by id
        # See https://github.com/thin-edge/thin-edge.io/issues/2616
        publish_message("c8y/s/us", "501,c8y_SoftwareUpdate")
        time.sleep(1)
        log.info("User did not confirm the operation")
        sys.exit(EXIT_NO_CONFIRM)
except KeyboardInterrupt:
    log.info("Stopping...")
    sys.exit(EXIT_ERROR)

sys.exit(EXIT_OK)
