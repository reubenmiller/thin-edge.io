const utf8 = new TextDecoder();

export function onMessage(message, context) {
    const [key, ...rest] = `${utf8.decode(message.payload)}`.split("=");
    const value = rest.join("=");
    if (key && value) {
        if (key == "device.id") {
            console.log(`key=${key}, value=${value}`);
            context.mapper.set(`${key}`, value);
        }
    }
    return [];
}