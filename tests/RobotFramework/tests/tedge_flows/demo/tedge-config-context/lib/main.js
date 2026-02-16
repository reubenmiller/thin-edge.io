export function onMessage(message, context) {
    const [key, ...rest] = `${message.payload}`.split("=");
    const value = rest.join("=");
    if (key && value) {
        if (key == "device.id") {
            console.log(`key=${key}, value=${value}`);
            context.mapper.set(`${key}`, value);
        }
    }
    return [];
}