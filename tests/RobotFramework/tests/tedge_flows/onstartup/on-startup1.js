export function onStartup(_time, context) {
    context.mapper.set("on-startup.js", `on_startup 1`)
    let msg = context.mapper.get("on-startup.js")

    console.log(msg)
    return { topic: "test/onstartup", payload: msg }
}

export function onMessage(message, _context) {
    let msg = `on_message 1`
    console.log(msg)
    return { topic: "test/onstartup", payload: msg }
}

