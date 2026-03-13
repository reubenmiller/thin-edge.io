export function onStartup(time, context) {
        return [{
                topic: "onstartup-health/step1",
                payload: JSON.stringify({text:'step 1 startup message', time: time.toISOString()}),
                mqtt: {retain:true},
        }, {
                topic: "te/device/main///e/startup",
                payload: JSON.stringify({text:'step 1 startup message'}),
        }]
}

export function onMessage(message, context) {
        return [];
}