export function onMessage(message, context) {
    const messageType = message.topic.split("/").slice(-1)[0];
    const source = context.mapper.get("device.id") || "";
    if (!source) {
        console.log(`Skipping message as the source could not be determined`);
        return [];
    }

    const count = context.script.get("count") || 1;
    context.script.set("count", count + 1);

    const payload = JSON.parse(message.payload);

    console.log(`Processing message`, {payload});
    
    // remove the text from the payload
    const { text, ...properties } = payload;
    return [{
        topic: "c8y/mqtt/out/te/v1/events",
        payload: JSON.stringify({
            ...properties,
            logMessage: `${text || "test event"} (from mqtt-service)`,
            tedgeFlowInstanceMessageCount: count,
            type: messageType,
            payloadType: "event",
            source,
        }),
    }];
}