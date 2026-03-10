export function onMessage(message, context) {
  const payload = JSON.parse(new TextDecoder("utf8").decode(message.payload));
  console.log(`Payload Raw: ${JSON.stringify(payload)}`);
  delete payload["source"];

  const result = [];
  const payloadType = payload["payloadType"];
  const externalId = message.clientID;

  if (payloadType == "telemetry") {
    result.push({
      cumulocityType: "measurement",
      externalSource: [{ "type": "c8y_Serial", "externalId": externalId }],
      payload: {
        "time": new Date(),
        "type": "c8y_TemperatureMeasurement",
        "c8y_Steam": {
          "Temperature": {
            "unit": "C",
            "value": payload["sensorData"]["temp_val"]
          }
        }
      },
    });
  } else {
    const eventType = payload["type"] || "c8y_ErrorEvent";
    result.push({
      cumulocityType: "event",
      externalSource: [{ "type": "c8y_Serial", "externalId": externalId }],
      payload: {
        ...payload,
        "time": new Date(),
        "type": eventType,
        "dataID": createRandomString(32),
        "text": payload["logMessage"] || "no log message provided",
      },
    });
  }
  return result;
}

function createRandomString(length) {
  const chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  let result = "";
  for (let i = 0; i < length; i++) {
    result += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return result;
}




