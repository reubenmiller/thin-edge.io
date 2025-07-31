
function fahrenheitToCelsius(f) { return (f - 32) * 5 / 9; }
function psiToBar(psi) { return psi * 0.0689476; }

export function onMessage(message) {
    const payload = JSON.parse(message.payload);
    if (typeof payload.temperature === "number") {
        return {
            topic: "te/device/main///m/temperature",
            payload: JSON.stringify({
                temperature: fahrenheitToCelsius(payload.temperature),
            }),
        };
    } else if (typeof payload.pressure === "number") {
        return {
            topic: "te/device/main///m/pressure",
            payload: JSON.stringify({
                pressure: psiToBar(payload.pressure),
            }),
        };
    }
}
