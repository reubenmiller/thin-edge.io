export function onStartup(time, context) {
    context.mapper.set("on-startup.js", "hello from on-startup.js")
    let hello = context.mapper.get("on-startup.js")
    console.log(hello)
    return { topic: "test/onstartup", payload: hello }
}
