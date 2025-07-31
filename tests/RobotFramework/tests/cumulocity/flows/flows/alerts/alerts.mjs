
export function onMessage(message, {high=80, warn=60}) {
    const payload = JSON.parse(message.payload);
    if (payload.temperature > high) {
        return {
            topic: "te/device/main///a/overheat",
            retain: true,
            payload: JSON.stringify({
                text: `temperature is > ${high}°C`,
                severity: "critical",
                value: payload.temperature,
            }),
        };
    } else if (payload.temperature > warn) {
        return {
            topic: "te/device/main///a/overheat",
            retain: true,
            payload: JSON.stringify({
                text: `temperature is > ${warn} but lower than ${high}°C`,
                severity: "major",
                value: payload.temperature,
            }),
        };
    } else {
        // clear
        return {
            topic: "te/device/main///a/overheat",
            retain: true,
            payload: "",
        };
    }
}
