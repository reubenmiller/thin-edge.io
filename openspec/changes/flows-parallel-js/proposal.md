## Why

Flows with JavaScript steps can use at most one CPU core,
however many flows are loaded and however many cores the device has.
On a 4-CPU Linux container, 8 independent flows reach the same throughput as 1,
and the process never uses more than about 1.2 cores
(evidence in [ADR 0013](../../../design/decisions/0013-flows-parallel-js.md)).
Running flows in parallel naively breaks flows that share `context.mapper`:
in a measured run, 499 of 500 cross-flow reads saw the wrong value,
and half of the shared counter increments were lost.

## Proposed solution

A user enables parallel execution with one setting:

```sh
tedge config set flows.workers auto
```

The mapper then runs JavaScript on one runtime per CPU core available to it
(or on a fixed number of runtimes, with `tedge config set flows.workers 4`),
and independent flows process messages in parallel.
No flow definition or script needs to change.

The mapper decides how each flow may run by reading its steps:

```js
// Independent: never mentions `mapper`, runs fully in parallel.
export function onMessage(message, context) {
    const count = (context.flow.get("count") || 0) + 1;
    context.flow.set("count", count);
    return [{ topic: "out", payload: `${count}` }];
}
```

```js
// Reader: only calls context.mapper.get or context.mapper.keys.
// Runs in parallel with other readers, after every earlier write.
export function onMessage(message, context) {
    const entity = context.mapper.get(entityId(message.topic));
    // ...
}
```

```js
// Writer: any other use of context.mapper.
// Runs after every earlier reader and writer call has finished.
export function onMessage(message, context) {
    context.mapper.set(entityId(message.topic), JSON.parse(message.payload));
    return [];
}
```

A flow normally processes one message at a time, so a single busy flow is still limited to one core.
Its author can opt in to processing several messages at once,
when the flow keeps no state across messages, or only per-topic state,
and has no `onInterval` function:

```toml
# flow.toml
workers = 4          # up to 4 copies of this flow, capped by flows.workers
partition = "topic"  # messages with the same topic always go to the same copy

[input.mqtt]
topics = ["te/+/+/+/+/m/+"]

[[steps]]
script = "measurements.js"
```

Values read from or written to `context.mapper` by flows are the same as with one worker.
Writes made outside the flows actor, such as entity registrations stored by the Cumulocity mapper,
are not ordered against flows, as today.
If a script accesses `context.mapper` in a way its class does not allow
(for example through a computed property name),
the mapper logs a warning that names the flow,
and moves the flow to the stricter class.

## What Changes

- New `flows.workers` setting: the number of QuickJS runtimes used by a mapper's flows,
  or `auto` for one per CPU core available to the mapper.
  Default `1`, which keeps the current single-runtime, one-message-at-a-time behavior.
- With `flows.workers > 1`:
  - Each flow is pinned to one runtime, unless it opts in to several copies (below).
    All its steps and its `onMessage`, `onInterval` and `onStartup` calls run there.
  - Flows process MQTT messages concurrently.
    Messages of a flow with one copy are still processed one at a time, in arrival order.
  - Each flow is classified as independent, reader or writer of `context.mapper`
    from its step sources and built-in transformers.
    Calls are ordered by class so that every `context.mapper` access happens in arrival order.
  - A misclassified access is detected at run time, logged, and corrects the flow's class.
  - Output messages are published in the order sequential processing would produce.
    MQTT and file outputs are written only by the flows actor, one result at a time,
    so flows writing to the same file never interleave.
- New optional `workers` and `partition` keys in a flow definition.
  `workers = N` runs the flow as `min(N, flows.workers)` copies on different runtimes,
  each with its own module state;
  `partition = "topic"` sends all messages of a topic to the same copy.
  A flow with a periodic step (`onInterval` or a periodic built-in) cannot set `workers` above 1.
  Copies of a writer flow are still ordered by the `context.mapper` rules,
  so `workers` only speeds up independent and reader flows.
- **BREAKING** (only when `flows.workers > 1`): flows on different runtimes no longer share `globalThis`.
  A flow whose steps use `globalThis` for anything other than a standard built-in
  (such as `globalThis.TextDecoder`) is loaded on the first runtime and classified as a writer,
  so flows that share state through `globalThis` keep working.
- With `flows.workers > 1`, built-in steps that react to context updates (`on_context_update`,
  for example `cache-early-messages`) are notified once per pipeline window instead of once per message.
- The interrupt time budget for runaway scripts becomes per runtime,
  and is reset when each call starts rather than when it is queued.
- Reloading a script re-creates only the runtime that owns it.

## Capabilities

### New Capabilities

- `flows-parallel-execution`: the `flows.workers` setting, the runtime pool,
  pinning flows to runtimes, concurrent processing of MQTT messages,
  per-flow ordering, output ordering and writing, the per-runtime interrupt budget,
  and the per-flow `workers` and `partition` keys.
- `flows-context-ordering`: classification of flows as independent, reader or writer,
  the ordering rules that keep `context.mapper` accesses in arrival order,
  run-time detection of misclassified access, and the `globalThis` rule.

### Modified Capabilities

None. No existing spec in `openspec/specs/` covers tedge flows.

## Impact

- `crates/extensions/tedge_flows`:
  `js_runtime.rs` (runtime pool), `runtime.rs` (lanes and scheduler),
  `actor.rs` (pipelined input), `flow.rs` and `steps.rs` (classification),
  `transformers/mod.rs` (built-in transformers declare their context access),
  `config.rs` (flow `workers` and `partition` keys),
  `js_lib/kv_store.rs` (access check), `lib.rs` (`FlowsMapperConfig::with_js_workers`).
- `crates/common/tedge_config`: new `flows.workers` key, with a `WorkerCount` type accepting `auto`.
- `crates/core/tedge_mapper` and `tedge flows test`: pass `flows.workers` to the flows actor.
- Memory: each extra runtime costs about 150 KB at idle.
  `flows.memory.heap_size` applies per runtime, so the worst-case JS heap is
  `flows.workers × heap_size`.
- Documentation: `docs/src/extend/flows.md` describes `flows.workers`,
  the classification, the `globalThis` change,
  and the flow `workers` and `partition` keys, with an example of state that breaks with copies.
