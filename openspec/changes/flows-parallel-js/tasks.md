## 1. Runtime pool (prototype exists on `perf/flows-parallel-js`)

- [x] 1.1 Add `JsRuntimeConfig::workers` and hold one QuickJS runtime per worker in `JsRuntime`, each served by a tokio task
- [x] 1.2 Pin each flow to the runtime with the fewest flows, and look up the owning runtime in `call_function`
- [x] 1.3 Replace the global `TIME_CREDITS` with a per-runtime counter, reset by the worker when a call starts
- [x] 1.4 Re-create only the owning runtime on reload, and reload its other modules
- [x] 1.5 Release a removed flow's runtime slot so the load counts stay accurate
- [x] 1.6 Test: a runaway script on one runtime times out while a flow on another runtime keeps processing
- [x] 1.7 Test: reloading a flow keeps module state of flows on other runtimes

## 2. Pipelined lanes (prototype exists, needs ordering rules)

- [x] 2.1 Add `MessageProcessor::on_messages` with one lane per flow, at most one call in flight per lane, and results sorted by (arrival, flow order)
- [x] 2.2 Take further MQTT messages from the mailbox with `recv_noblock` as calls complete, up to 1024 per window, stopping at the first non-MQTT input
- [x] 2.3 Keep `flows.workers = 1` on the existing `on_message` path
- [x] 2.4 Give built-in flows their own lanes, ordered by their declared class, instead of the shared serial lane
- [x] 2.5 Test: 4 stateful flows produce the same outputs, in input order, with 1 and 4 runtimes

## 3. Classification

- [x] 3.1 Add a `ContextAccess { Independent, Read, Write }` enum, ordered from weakest to strongest
- [x] 3.2 Implement the source scanner for `mapper` (reader only when every occurrence is followed by `.get(` or `.keys(`) and unit-test each rule in the `flows-context-ordering` spec
- [x] 3.3 Implement the `globalThis` rule: any use other than `globalThis.<standard built-in>` sets a shared-globals flag
- [x] 3.4 Add `Transformer::context_access()`, defaulting to `Write`, and declare it for every transformer in `tedge_flows/src/transformers`
- [x] 3.5 Compute a flow's class as the strongest class of its steps, and recompute it on reload
- [x] 3.6 Load flows with the shared-globals flag on the first runtime and classify them as writer

## 4. Reader/writer ordering

- [x] 4.1 Track, per window, the earliest unfinished reader and writer positions, including queued calls
- [x] 4.2 Start a lane's head call only when its class allows it, and re-check waiting lanes each time a call completes
- [x] 4.3 Test: a reader flow reads the value a writer flow stored for the earlier message, identical to 1 runtime (500/500)
- [x] 4.4 Test: a fast writer never makes a slow reader see a value written for a later message
- [x] 4.5 Test: two writer flows incrementing one counter 500 times each end at 1000
- [x] 4.6 Test: 4 reader flows with no writer run concurrently (more than one call in flight)
- [x] 4.7 Test: `units` then `measurements` built-in flows keep their order with 4 runtimes

## 5. Run-time access check

- [x] 5.1 Carry the calling flow in `FlowContext::Mapper`, and keep flow classes in a table shared by the scheduler and the context functions
- [x] 5.2 On an access the flow's class does not allow, complete it, warn once per flow, and raise the class in the table
- [x] 5.3 Test: a script using `context["map" + "per"].set(...)` logs one warning and its later calls are ordered as writer

## 6. Per-flow copies and outputs

- [x] 6.1 Parse the flow `workers` key (positive integer, default 1) and `partition` key (`topic` only) in `config.rs`, with errors naming the flow and key
- [x] 6.2 Refuse to load a flow with `workers > 1` and a periodic step, naming the step
- [x] 6.3 Load `min(workers, flows.workers)` copies of the flow on different runtimes, each with its own transformer instances, sharing `context.flow` and `context.script`, and run `onStartup` on each copy
- [x] 6.4 Make each copy a lane, and route messages to the first free copy or by `hash(topic) % copies`
- [x] 6.5 Re-create all copies when the flow or `flows.workers` changes
- [x] 6.6 Test: a stateless CPU-bound flow with `workers = 4` scales on 4 runtimes (single-flow case in the harness)
- [x] 6.7 Test: with `partition = "topic"`, per-topic order and per-topic counters in module state are correct
- [x] 6.8 Test: `workers = 8` with `flows.workers = 2` runs 2 copies; `workers = 0` and `partition = "payload"` fail to load
- [x] 6.9 Test: a writer flow with `workers = 4` keeps a `context.mapper` counter exact
- [x] 6.10 Test: two flows writing the same output file produce the same lines, in the same order, with 1 and 4 runtimes

## 7. Configuration and documentation

- [x] 7.1 Add `flows.workers` (default `1`, reject `0`) to `tedge_config` next to `flows.memory`
- [x] 7.1a Accept `flows.workers = auto`, resolved with `std::thread::available_parallelism()` (default stays `1`)
- [x] 7.2 Pass `flows.workers` to the flows actor in `tedge_mapper` (`with_js_workers`) and to `tedge flows test`
- [x] 7.3 Document `flows.workers`, the classification, the per-runtime heap limit, the `globalThis` change, and the flow `workers` and `partition` keys (with an example of state that breaks with copies) in `docs/src/extend/flows.md`
- [x] 7.4 Update ADR 0013: replace the `concurrency.group` section with a link to this change

## 8. Validation

- [x] 8.1 Run all `tedge_flows` tests with `flows.workers = 1` and with the default temporarily set to 4
- [x] 8.2 Re-run the `flows_throughput` harness on Linux (4 CPUs) for 1, 2 and 4 runtimes, including a mix of reader and writer flows, and record the results in the ADR
- [ ] 8.3 Run the `tedge_flows` and Cumulocity flows system tests with `flows.workers = 4` against a local broker only
- [x] 8.4 Run `just format` and `just check`
