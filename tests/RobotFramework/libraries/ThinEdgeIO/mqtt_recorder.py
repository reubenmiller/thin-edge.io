"""MQTT message recorder

Used on Windows in place of the mosquitto logging plugin (which provides the
`mqtt-log` command on Linux), as it has no Windows build.

The recorder subscribes to all topics ("#") and keeps every message it receives
in memory, along with the time it was received. Retained messages delivered on
subscription are recorded with the time they were received (i.e. when the
subscription was made). After a reconnection, the retained messages are
delivered and recorded again, with the time of the new subscription.
"""

import base64
import logging
import os
import re
import threading
import time
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

log = logging.getLogger(__name__)

# Message type of the recorded messages, using the mqtt-log naming.
# The recorder only sees the messages published by the clients.
MESSAGE_TYPE = "publish_in"


def topic_matches(topic_filter: str, topic: str) -> bool:
    """Check if a topic matches an MQTT topic filter (supporting + and # wildcards)

    Like an MQTT broker, topics starting with "$" are not matched
    by a filter starting with a wildcard.
    """
    if topic_filter == topic:
        return True

    if topic.startswith("$") and topic_filter[:1] in ("+", "#"):
        return False

    filter_levels = topic_filter.split("/")
    topic_levels = topic.split("/")
    for i, level in enumerate(filter_levels):
        if level == "#":
            # matches the parent level and any number of child levels
            return i == len(filter_levels) - 1
        if i >= len(topic_levels):
            return False
        if level not in ("+", topic_levels[i]):
            return False
    return len(filter_levels) == len(topic_levels)


def filter_messages(
    records: List[Dict[str, Any]],
    topic: Optional[str] = None,
    message_pattern: Optional[str] = None,
    date_from: Optional[float] = None,
    date_to: Optional[float] = None,
) -> List[Dict[str, Any]]:
    """Filter recorded messages with the same semantics as the mqtt-log arguments

    Args:
        records (List[Dict[str, Any]]): Recorded messages
        topic (str, optional): Topic filter (supports MQTT wildcards). Like `--topic`
        message_pattern (str, optional): Regular expression searched for in the payload.
            Like `--payload-matches`
        date_from (float, optional): Unix timestamp (inclusive). Like `--from`
        date_to (float, optional): Unix timestamp (inclusive). Like `--to`

    Returns:
        List[Dict[str, Any]]: Matching records in the order they were received
    """
    pattern = re.compile(message_pattern) if message_pattern else None
    matches = []
    for record in records:
        if date_from is not None and record["timestamp"] < date_from:
            continue
        if date_to is not None and record["timestamp"] > date_to:
            continue
        if topic and not topic_matches(topic, record["topic"]):
            continue
        if pattern and not pattern.search(record["payload"]):
            continue
        matches.append(record)
    return matches


def to_mqtt_log_entry(record: Dict[str, Any]) -> Dict[str, Any]:
    """Convert a record to the format of an `mqtt-log --output ndjson` entry"""
    timestamp = datetime.fromtimestamp(record["timestamp"], tz=timezone.utc)
    payload_bytes = record.get("payload_bytes", record["payload"].encode("utf8"))
    return {
        "timestamp": timestamp.strftime("%Y-%m-%dT%H:%M:%S.%fZ") + "+0000",
        "timestamp_unix": record["timestamp"],
        "type": MESSAGE_TYPE,
        "topic": record["topic"],
        "qos": record["qos"],
        "retain": int(record["retain"]),
        "payloadlen": len(payload_bytes),
        "payload": record["payload"],
        "payload_base64": base64.b64encode(payload_bytes).decode("ascii"),
    }


class MQTTRecorder:
    """Record all messages published on an MQTT broker"""

    def __init__(self, host: str = "127.0.0.1", port: int = 1883):
        self.host = host
        self.port = port
        self._records: List[Dict[str, Any]] = []
        self._lock = threading.Lock()
        self._connected = threading.Event()
        self._subscribed_at: Optional[float] = None
        self._client = None

    @property
    def is_running(self) -> bool:
        """Is the recorder running"""
        return self._client is not None

    def start(self, timeout: float = 10.0):
        """Connect to the broker and subscribe to all topics

        Raises:
            ConnectionError: The subscription could not be made within the timeout
        """
        if self._client is not None:
            return

        # pylint: disable=import-outside-toplevel
        import paho.mqtt.client as mqtt

        client = mqtt.Client(
            client_id=f"robot-mqtt-recorder-{os.getpid()}-{uuid.uuid4().hex[:8]}",
            clean_session=True,
        )
        client.on_connect = self._on_connect
        client.on_subscribe = self._on_subscribe
        client.on_disconnect = self._on_disconnect
        client.on_message = self._on_message
        client.connect_async(self.host, self.port, keepalive=30)
        client.loop_start()
        self._client = client

        if not self._connected.wait(timeout):
            self.stop()
            raise ConnectionError(
                f"MQTT recorder could not subscribe to {self.host}:{self.port} "
                f"within {timeout}s. Check that the MQTT broker is running"
            )
        log.info("MQTT recorder subscribed to '#' on %s:%s", self.host, self.port)

    def stop(self):
        """Disconnect from the broker. The recorded messages are kept"""
        client, self._client = self._client, None
        if client is None:
            return
        try:
            client.disconnect()
        finally:
            client.loop_stop()
            self._connected.clear()

    def clear(self):
        """Remove all recorded messages"""
        with self._lock:
            self._records.clear()

    def records(self) -> List[Dict[str, Any]]:
        """Get a copy of the recorded messages

        Each record has the keys: topic, payload (str), retain, qos, timestamp (unix float)
        """
        with self._lock:
            return list(self._records)

    def record(
        self,
        topic: str,
        payload: bytes,
        qos: int = 0,
        retain: bool = False,
        timestamp: Optional[float] = None,
    ):
        """Add a message to the records"""
        entry = {
            "topic": topic,
            "payload": payload.decode("utf8", errors="replace"),
            "payload_bytes": payload,
            "retain": bool(retain),
            "qos": qos,
            "timestamp": time.time() if timestamp is None else timestamp,
        }
        with self._lock:
            self._records.append(entry)

    # pylint: disable=unused-argument
    def _on_connect(self, client, userdata, flags, rc):
        if rc != 0:
            log.warning("MQTT recorder connection refused. rc=%s", rc)
            return
        # (re)subscribe after each connection, as the session is not persisted
        client.subscribe("#", qos=2)

    def _on_subscribe(self, client, userdata, mid, granted_qos):
        self._subscribed_at = time.time()
        self._connected.set()

    def _on_disconnect(self, client, userdata, rc):
        self._connected.clear()
        if rc != 0:
            log.warning("MQTT recorder disconnected unexpectedly. rc=%s", rc)

    def _on_message(self, client, userdata, message):
        # The broker only sets the retain flag on the messages delivered
        # because of a new subscription, so they get the subscription time
        self.record(
            message.topic,
            message.payload,
            qos=message.qos,
            retain=message.retain,
            timestamp=self._subscribed_at if message.retain else None,
        )
