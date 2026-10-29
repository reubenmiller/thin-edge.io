# Parallel JavaScript execution in tedge flows

* Date: __2026-10-28__
* Status: __Proposed__

The detailed requirements, design and tasks are in the OpenSpec change
[`flows-parallel-js`](../../openspec/changes/flows-parallel-js/proposal.md).
This record keeps the evidence, the decision and the measurements behind it.

## Background

Users reported that flows using JavaScript steps
cannot use more than one CPU core,
and that the flows engine becomes a bottleneck on multi-core devices.

### Evidence

The hypothesis was checked with a throughput harness,
`crates/extensions/tedge_flows/examples/flows_throughput.rs`.
The harness runs the real `FlowsMapper` actor against an in-memory MQTT peer
(no broker, no cloud connection),
loads `FLOWS` independent flows that each run the same JS step,
and pushes the same total number of messages through them.
`WORK` adds a CPU-bound loop to each `onMessage` call
to emulate heavier user logic.

On the current `main` (10-core macOS host, 160,000 messages per run):

| work | 1 flow | 2 flows | 4 flows | 8 flows | CPU used |
|------|--------|---------|---------|---------|----------|
| 0    | 29,073 msg/s | 29,177 | 29,047 | 28,836 | 1.2 cores |
| 200  | 14,791 msg/s | 14,671 | 14,675 | 14,461 | 1.15 cores |
| 2000 | 2,870 msg/s  | 2,814  | 2,846  | 2,801  | 1.0 cores |

* Throughput stays flat as independent flows are added.
* CPU usage converges to exactly one core as the share of time spent in JS grows,
  although 10 cores are available.
* Per-thread sampling (`ps -M`) during a `work=2000` run
  shows one thread at 99% CPU and every other thread at about 1%.

The hypothesis is confirmed.

### Root cause

JS execution is serialized at two levels.

1. __One JS runtime per mapper.__
   `JsRuntime` (`js_runtime.rs`) owns a single QuickJS `AsyncRuntime` and `AsyncContext`.
   One tokio task serves every `call_function` request through an `mpsc` channel,
   so at most one script runs at any time, whatever the number of flows.
2. __One message at a time in the actor.__
   `FlowsMapper::run` (`actor.rs`) awaits each MQTT message to completion,
   and `MessageProcessor::on_message` (`runtime.rs`) awaits each matching flow in turn.
   Even with several JS runtimes,
   nothing would be in flight concurrently.

The global `TIME_CREDITS` interrupt counter also assumed a single runtime.

### Why parallel execution needs ordering

A first prototype ran every JS flow in its own lane, with no ordering between lanes.
It scaled (see [Validation](#validation)),
but broke flows that share state through `context.mapper`.
Temporary experiments, run with 1 and then 4 runtimes:

| Experiment | 1 runtime | 4 runtimes, no ordering |
|------------|-----------|-------------------------|
| A writer flow stores each message index in `context.mapper`; a reader flow, fed interleaved messages, reads it back | 500/500 reads see the matching value | 499/500 reads see another message's value, mostly a later one |
| Two flows each increment one `context.mapper` counter, 1000 increments in total | 1000 | 505: about half the increments are lost |
| One flow increments a `context.flow` counter | 500/500 | 500/500 |
| 4 flows count messages in module state and `context.flow`; `onInterval` reports both, through the real actor | consistent | consistent: 100/100, 200/200, 300/300 |

* __Within one flow__, nothing broke.
  A flow's messages ran one at a time, ticks were handled only after the pipeline drained,
  and all calls of a flow ran on the runtime it was pinned to.
* __Across flows__, through `context.mapper`, there were two failure modes:
  reads saw a value written for an earlier or a later message,
  and a `get` then `set` (read-modify-write) lost updates.
  Each `get` or `set` is atomic, but the pair is not.

Sharing `context.mapper` between flows is a documented, common pattern:
one flow registers entities, and other flows look them up
(for example `register-entities.js` with `custom-measurements.js`
in the system tests).
Parallel execution must keep it working without changes to existing flows.

## Decision

Run JavaScript on a pool of QuickJS runtimes,
process flows concurrently,
and order calls automatically so that every `context.mapper` access
happens in the order sequential processing would produce.
No flow definition or script needs to change.
Parallelism within a single flow is an explicit opt-in.

### 1. A pool of JS runtimes

The new `flows.workers` setting selects the number of runtimes:

```sh
tedge config set flows.workers 4
```

* `JsRuntime` holds `workers` QuickJS runtimes, each served by its own tokio task.
* Each flow is pinned to the runtime with the fewest flows when it is first loaded.
  All its steps, and its `onMessage`, `onInterval` and `onStartup` calls, run there,
  so module state (`let count = 0` at the top of a script) behaves as today.
* The interrupt budget for runaway scripts becomes per runtime,
  and is reset by the worker just before it runs each call.
  Previously the global counter was reset when a request was sent,
  so queued requests could reset each other's budget.
* Hot reload re-creates only the runtime owning the reloaded module.

### 2. Pipelined lanes in the actor

When `workers > 1`, the actor hands an MQTT message to `MessageProcessor::on_messages`
together with a non-blocking source of the next queued MQTT messages
(`MessageReceiverNoblock::recv_noblock`).

* Each flow copy has a __lane__: a FIFO of the messages it must process,
  with at most one call in flight.
* Each time a lane completes a call,
  more queued messages are taken from the mailbox and routed,
  so a busy lane never holds back the others.
* After 1024 messages,
  or as soon as a non-MQTT input is queued (tick, file-system event, command output),
  the pipeline drains and the actor handles that input,
  so intervals, reloads and context updates are never starved.
* Results are sorted back into the order sequential processing would produce
  (by message arrival, then flow order) before they are published.

### 3. Automatic ordering of `context.mapper`

Each flow is classified when it is loaded:

* __Independent__: no step mentions `mapper`.
* __Reader__: every mention of `mapper` is followed by `.get(` or `.keys(`.
* __Writer__: any other use, including aliasing, destructuring,
  and the word `mapper` in comments or strings.

JS steps are classified from their source text.
A step is a single ES module and the runtime has no module loader,
so the source is the whole program,
and `context` is a call argument, so the mapper scope can only be reached by spelling `mapper`.
Built-in transformers declare their access with `Transformer::context_access()`,
which defaults to writer.
A flow takes the strongest class of its steps.

Calls are ordered by position (message arrival, flow order).
A lane's next call starts when:

* independent: its lane is free;
* reader: no earlier writer call is unfinished;
* writer: no earlier reader or writer call is unfinished.

Calls queued but not yet started count as unfinished.
Readers run concurrently with each other and with independent flows;
a writer runs alone with respect to earlier mapper-scope calls.
The result is the same as sequential execution for every `context.mapper` read and write.
The scheduler cannot deadlock:
the unfinished call with the earliest position is always at the head of its lane,
and no rule makes it wait for a later call.

Each `context.mapper` access is checked at run time against the calling flow's class.
A misclassified access (for example through `context["map" + "per"]`) completes,
logs a warning once per flow, and moves the flow to the stricter class for its later calls.

Writes made outside the flows actor, such as entity registrations stored by the Cumulocity mapper,
are not ordered against flows, as today.

### 4. `globalThis`

Runtimes do not share `globalThis`.
ES modules run in strict mode, so a script can only create a global by naming `globalThis`.
A flow whose steps use `globalThis` for anything other than a standard built-in
(such as `globalThis.TextDecoder`)
is loaded on the first runtime and classified as writer,
so flows that share state through `globalThis` keep working.

### 5. Opt-in copies of a single flow

A flow normally processes one message at a time,
so a single busy flow stays limited to one core.
A flow that keeps no state across messages, or only per-topic state,
can opt in to several copies:

```toml
workers = 4          # up to 4 copies, capped by flows.workers
partition = "topic"  # messages with the same topic always go to the same copy
```

* The flow runs as `min(workers, flows.workers)` copies on different runtimes,
  each with its own module state.
  All copies share `context.flow` and `context.script`.
* Without `partition`, a message goes to the first free copy.
  With `partition = "topic"`, it goes to the copy selected by a hash of its topic,
  which keeps per-topic order and per-topic state.
* A flow with a periodic step (`onInterval` or a periodic built-in) cannot set `workers` above 1:
  `onInterval` on one copy would see only that copy's state,
  and running it on every copy would multiply its outputs.
* Copies follow the same `context.mapper` rules as separate flows,
  so copies of a writer flow are still ordered against each other.

This is opt-in because no static check can prove a script stateless:
any top-level `let` may be a counter or a cache.

### 6. Outputs are written only by the actor

JS steps have no filesystem API.
MQTT and file outputs of all flows are written by the actor task,
one result at a time, in the sorted order,
and each file result is appended and flushed before the next.
Flows writing to the same file therefore need no coordination,
and lines keep the single-runtime order.

### 7. Default unchanged, with `auto` to opt in

`flows.workers` accepts a positive integer or `auto`.
`auto` uses one runtime per CPU core available to the mapper,
as reported by `std::thread::available_parallelism()`,
which takes into account CPU affinity and cgroup CPU quotas on Linux.
It is not the default yet: see open item 1.

With `flows.workers = 1` (the default),
the actor calls the existing `MessageProcessor::on_message` path,
so behavior and performance are the same as today.
The pipelined path costs 6% to 15% per message with one runtime,
so it is only used when there is something to run in parallel.

## Ordering and consistency

| Guarantee | `workers = 1` | `workers > 1` |
|-----------|---------------|---------------|
| Messages of one flow are processed in arrival order | Yes | Yes, for flows with one copy; per topic with `partition = "topic"` |
| Module state of a script is consistent | Yes | Yes, for flows with one copy; per copy otherwise |
| `onMessage`, `onInterval` and `onStartup` of the same flow never overlap, and share state | Yes | Yes (flows with `onInterval` cannot have copies) |
| Published output order matches sequential processing | Yes | Yes, within a pipeline window |
| Lines written to a shared file output never interleave | Yes | Yes |
| A flow sees a `context.mapper` update made by another flow for an earlier message, and never one for a later message | Yes | Yes |
| Read-modify-write of a `context.mapper` key by two flows keeps both updates | Yes | Yes |
| Read-modify-write of `context.flow` by copies of one flow keeps both updates | Not applicable | No: only use `workers` for flows without such state |
| Built-in flows see each other's context updates in message order (for example `units` then `measurements`) | Yes | Yes, built-ins are writers unless they declare otherwise |
| Scripts can share state through `globalThis` | Yes, by accident | Yes, those flows share the first runtime |
| `on_context_update` handlers (for example `cache-early-messages`) are notified after each message | Yes | No: once per pipeline window |
| A `context.mapper` access through a computed property name is ordered | Yes | Not the first one: it is detected, logged, and later calls are ordered |

## Alternatives considered

The performance alternatives were measured with the same harness.

### Batch with a barrier (rejected)

The actor drains the queued messages into a batch,
runs the lanes of that batch concurrently,
then waits for all of them before taking the next batch.

With 4 workers and 4 flows, this reached 1.2× at best.
Instrumentation showed why:
batches averaged 19.9 messages, but the longest lane held 16.4 of them,
because MQTT input arrives in per-topic bursts.
Each batch waited for its busiest lane.
Increasing the batch size to 256 only reached 1.35×.

### One OS thread per JS runtime (rejected)

Running each runtime on a dedicated thread
isolates long scripts from the tokio threads used by other actors.
On Linux, it made a single busy flow 28% to 56% slower than today,
because every call pays a cross-thread wake-up.
Tokio tasks, as used today, avoid that cost:

| Linux, 4 CPUs | baseline | 4 dedicated threads | 4 tokio tasks |
|---------------|----------|---------------------|---------------|
| work=200, 1 flow   | 21,989 | 9,632 (−56%)  | 24,201 (+10%) |
| work=200, 8 flows  | 20,758 | 31,669        | 47,784 (2.3×) |
| work=2000, 1 flow  | 6,328  | 4,541 (−28%)  | 6,472 (+2%)   |
| work=2000, 8 flows | 6,241  | 14,184        | 15,367 (2.5×) |

The trade-off is that a long-running script blocks a tokio worker thread,
as it does today with one runtime.
With `workers` runtimes, up to `workers` tokio threads can be blocked at once,
so `workers` must stay below the number of tokio threads (the number of cores).

### Per-flow concurrency groups (rejected)

Each flow would declare a `concurrency.group`;
flows in the same group run in order, and flows without a group share the default group.
This is safe, but every flow must be edited to benefit,
which is not feasible across deployed devices.

### Serialize every flow that uses `context.mapper` (rejected)

Simpler than reader/writer ordering,
but flows that only look up entities, the most common kind, would lose all parallelism.

### Wait inside `context.mapper.get` and `set` (rejected)

The context functions are synchronous, so waiting for a call's turn would block its runtime.
When several flows share a runtime, a later call blocking there
would prevent an earlier call queued on the same runtime from running: a deadlock.

### Optimistic execution with re-execution on conflict (rejected)

Module state, such as a top-level counter, cannot be rolled back,
so re-running a call would change its results.

### Document the weaker guarantees only (rejected)

This would break the "register, then look up" pattern used by existing flows,
as soon as a user raises `flows.workers`.

### Automatic copies of stateless flows (rejected)

Running every flow as several copies would scale single-flow deployments,
but no static check can prove a script keeps no state across messages.
Copies are therefore an explicit per-flow choice.

## Validation

### With ordering and copies

Static `aarch64-unknown-linux-musl` builds of `main` (baseline) and of the implementation,
run in a Debian container limited to 4 CPUs, 80,000 messages per run.
Each value is the median of 3 runs.
`MAPPER` selects how the harness flows use `context.mapper`:
not at all (independent), `get` only (readers), or one writer flow and readers.

| work | flows | baseline | 1 runtime | 4 runtimes, independent | 4 runtimes, readers | 4 runtimes, 1 writer and readers |
|------|-------|----------|-----------|-------------------------|---------------------|----------------------------------|
| 200  | 4 | 26,968 | 27,020 | 62,493 (2.3×) | 57,047 (2.1×) | 30,165 (1.1×) |
| 200  | 8 | 25,008 | 26,053 | 61,650 (2.5×) | 56,575 (2.3×) | 34,699 (1.4×) |
| 2000 | 4 | 6,968  | 6,886  | 20,061 (2.9×) | 20,394 (2.9×) | 7,985 (1.15×) |
| 2000 | 8 | 6,885  | 6,748  | 20,087 (2.9×) | 20,844 (3.0×) | 9,604 (1.4×)  |

A single flow, with and without copies (`workers = 4` in the flow definition):

| work | baseline | 4 runtimes | 4 runtimes, 4 copies |
|------|----------|------------|----------------------|
| 200  | 27,409 | 29,618 | 72,112 (2.6×) |
| 2000 | 7,002  | 7,078  | 25,332 (3.6×) |

* With one runtime, throughput stays within 4% of `main`.
* Readers scale like independent flows: only writers are ordered against them.
* One writer limits the gain of the readers it is interleaved with to 1.1× to 1.4×,
  as each writer call waits for every earlier reader call, and the reverse.
* Copies let a single busy flow use 3.5 of the 4 cores.
* The first of the 3 runs was 10% to 60% slower than the other two in most rows,
  so single runs in a fresh container are not reliable.

### First prototype

The measurements below are from the first prototype:
a runtime pool and pipelined lanes, with every flow independent
and no `context.mapper` ordering or per-flow copies.

Static `aarch64-unknown-linux-musl` builds of `main` and of the prototype,
run in a Debian container limited to 4 CPUs, 80,000 messages per run:

| work | flows | baseline | 1 worker | 2 workers | 4 workers | speed-up (4 workers) |
|------|-------|----------|----------|-----------|-----------|----------------------|
| 0    | 1 | 38,972 | 36,618 | 42,413 | 42,905 | 1.10× |
| 0    | 4 | 36,910 | 35,933 | 57,980 | 64,991 | 1.76× |
| 0    | 8 | 35,585 | 35,052 | 58,059 | 64,669 | 1.82× |
| 200  | 1 | 23,186 | 23,356 | 27,085 | 28,488 | 1.23× |
| 200  | 4 | 22,589 | 22,644 | 38,190 | 50,111 | 2.22× |
| 200  | 8 | 22,899 | 22,367 | 40,948 | 57,722 | 2.52× |
| 2000 | 1 | 6,577  | 6,542  | 6,750  | 6,769  | 1.03× |
| 2000 | 4 | 6,877  | 6,713  | 10,905 | 16,460 | 2.39× |
| 2000 | 8 | 6,381  | 6,532  | 10,977 | 15,828 | 2.48× |

* With one worker, throughput stays within 3.5% of `main`,
  except one run 6% below (`work=0`, 1 flow), which was not investigated further.
* With 4 workers, throughput scales with the number of flows, up to 2.5× on 4 CPUs.
* A single flow does not slow down, and gains up to 23%,
  because the actor receives and publishes messages while the script runs.
  It cannot gain more without per-flow copies (`work=2000`, 1 flow: 1.03×).
* The process tops out at about 2.5 of 4 cores.
  The remaining serial work is in the actor:
  routing, stats, message conversion and publishing.

On the 10-core macOS host with 8 workers and 8 flows,
throughput rose from 2,801 to 10,691 msg/s at `work=2000` (3.8×, 5 cores used).

Idle memory (max RSS, 8 flows loaded, 1 message):
2.3 MB with 1 worker, 2.8 MB with 4 workers, 3.3 MB with 8 workers,
so each extra runtime costs about 130 to 150 KB.
The `flows.memory.heap_size` limit applies to each runtime,
so the worst-case JS heap grows to `workers × heap_size`.

### Tests

* A test, `concurrent_flows_produce_the_same_outputs_as_sequential_flows`,
  feeds the same 200 messages to 4 stateful flows (including a topic shared by all of them)
  with 1 and 4 workers.
  It checks that each flow produces identical outputs
  and that results are returned in input order.
* The `context.mapper` experiments above are permanent tests in `runtime.rs`,
  and give the same results with 4 runtimes as with 1.
  With the ordering rules disabled, 5 of them fail.
* Other tests cover per-topic partitioning, copies, the limits on `workers` and `partition`,
  a runaway script on another runtime, reloading, and the run-time access check.
* All 126 `tedge_flows` tests pass with 1 runtime, and with the default temporarily set to 4.

### Reproducing

```sh
# macOS or Linux host
FLOWS=8 MSGS=20000 WORK=200 JS_WORKERS=4 \
    cargo run --release -p tedge_flows --example flows_throughput

# 1 writer and 7 readers of context.mapper
FLOWS=8 MSGS=10000 WORK=200 JS_WORKERS=4 MAPPER=mixed \
    cargo run --release -p tedge_flows --example flows_throughput

# A single flow running as 4 copies
FLOWS=1 MSGS=80000 WORK=200 JS_WORKERS=4 FLOW_WORKERS=4 \
    cargo run --release -p tedge_flows --example flows_throughput

# Count cores used: user + sys time divided by elapsed time
/usr/bin/time -p target/release/examples/flows_throughput
```

## Open items

1. Make `auto` the default of `flows.workers` in a later release.
   The conditions are:
   the system tests pass with several runtimes;
   the memory cost is acceptable on small devices,
   since `flows.memory.heap_size` applies per runtime;
   and field experience shows no problem with the two remaining behavior changes
   for flows that do not set `workers`:
   the `globalThis` rule, and `on_context_update` notifications once per pipeline window.
   Capping `auto`, for example at 4, or at the number of loaded flows, is an option if memory is the concern.
2. Bound the pipeline window by time (for example 50 ms) as well as by message count,
   so tick latency under load does not depend on message rate.
3. Run `onInterval`, `onStartup` and process or file inputs through the lanes too;
   they still execute one flow at a time.
4. Reduce actor-side cost per message to lift the ceiling of about 3 cores on 4 CPUs:
   one stats counter per lane per window, and fewer message clones.
5. Declare the context access of the Cumulocity built-in transformers,
   which are ordered as writers until they do.
6. Validate on a real device (for example a 4-core Raspberry Pi)
   with mosquitto and the built-in Cumulocity flows,
   using a local MQTT broker rather than a cloud connection.
