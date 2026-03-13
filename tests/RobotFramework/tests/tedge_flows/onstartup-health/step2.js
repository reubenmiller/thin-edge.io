export function onStartup(time, context) {
        return [{
                topic: "onstartup-health/step2",
                payload: JSON.stringify({text:'step 2 startup message', time: time.toISOString()}),
                mqtt: {retain:true},
        }, {
                topic: "te/device/main///e/startup",
                payload: JSON.stringify({text:'step 2 startup message'}),
        }]
}

export function onMessage(message, context) {
        return [];
}