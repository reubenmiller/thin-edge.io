## Context

Flows with JavaScript steps are limited to one CPU core
([proposal](proposal.md), measurements in [ADR 0013](../../../design/decisions/0013-flows-parallel-js.md)).
The design is shaped by these constraints:

- No per-flow configuration: existing flows and scripts must keep working unchanged.
- `context.mapper` values seen by flows must be the same as with one runtime.
- Module state of a script, `context.flow` and `context.script` must stay consistent across
  `onMessage`, `onInterval` and `onStartup` calls of a flow.
- Devices are resource-constrained: idle memory per runtime must stay small,
  and `flows.workers = 1` must cost nothing compared with today.
- QuickJS runtimes are single-threaded.
  The `context.mapper` functions exposed to JS are synchronous and cannot await.

A prototype of the runtime pool and of per-flow lanes exists on branch `perf/flows-parallel-js`.
It runs every JS flow in its own lane without ordering between lanes,
which is what produced the measured `context.mapper` failures.
This design keeps the pool and the lanes, and adds the classification and the ordering rules.

## Goals / Non-Goals

**Goals:**

- Scale throughput with the number of flows, up to the number of runtimes.
- Keep every `context.mapper` read and write in the order sequential processing produces,
  without user input.
- Keep `flows.workers = 1` on today's code path.

**Non-Goals:**

- Parallel processing of messages within one flow by default.
  Flows keep state across messages, so this is only done when the flow sets `workers` (D10).
- Ordering of `context.mapper` writes made by other actors, such as the Cumulocity mapper.
  These are not ordered against flows today either.
- Running `onInterval`, `onStartup`, process inputs and file inputs concurrently.
  They keep running one flow at a time, after the pipeline drains.
- Changing the default of `flows.workers`.

## Decisions

### D1. A pool of runtimes served by tokio tasks

`JsRuntime` holds `workers` QuickJS runtimes.
Each runtime is served by a tokio task that receives calls through an `mpsc` channel,
as the single runtime is today.

Alternative: one dedicated OS thread per runtime.
It isolates long scripts from other actors,
but made a single busy flow 28% to 56% slower on Linux because every call pays a cross-thread wake-up.
Tokio tasks were faster in every measured case.
The cost is that a long script blocks a tokio worker thread, as it does today.

### D2. Flows are pinned to a runtime

A flow is assigned to the runtime with the fewest flows when it is first loaded,
and all its steps are loaded there.
Pinning keeps module state shared by all calls of a flow,
and keeps `onInterval` on the same runtime as `onMessage`.
Hot reload re-creates only the owning runtime and reloads its other modules.

### D3. Pipelined lanes rather than batches

The actor passes each MQTT message to `MessageProcessor::on_messages`
with a non-blocking source of the next queued MQTT messages.
Each flow has a lane: a FIFO of the messages it must process.
At most one call per lane is in flight.
When a call completes, the processor takes more queued messages from the mailbox,
up to 1024 per window, or until the next queued input is not an MQTT message.
Results are sorted by (arrival, flow order) before they are published.

Alternative: take a batch of queued messages, process the batch's lanes concurrently,
and wait for all lanes before the next batch.
This reached only 1.2× with 4 runtimes,
because MQTT input arrives in per-topic bursts and each batch waits for its busiest lane.

### D4. Classify flows from their source text

Each JS step is classified when it is loaded, from its source (rules in the `flows-context-ordering` spec).
The check is a scan for the identifier `mapper` and the token that follows it.

Classification by source text is sound for realistic scripts:

- A flow step is a single ES module. The flows runtime has no module loader,
  so there is no imported code to analyse.
- `context` is passed as an argument, so any use of the mapper scope spells `mapper`,
  except through computed property names.
- Every ambiguous form (aliasing, destructuring, `mapper` in comments or strings)
  is classified as writer, so a wrong guess costs parallelism, not correctness.

Built-in transformers receive the `FlowContextHandle` directly,
so the scan cannot apply to them.
They declare their access with a new `Transformer::context_access()` method,
which defaults to `ContextAccess::Write`.
Transformers defined outside `tedge_flows`, such as the Cumulocity built-in flows,
are therefore ordered as writers until they declare otherwise.

Alternatives:

- **Opt-in `concurrency.group` per flow.**
  Every flow must be edited to benefit, which is not feasible across deployed devices.
- **Detect access at run time only, starting every flow as independent.**
  The first access by each flow would be unordered, and could be wrong.
- **Start every flow as a writer and relax after observing it.**
  A flow that only writes the mapper scope occasionally,
  for example on a registration message, would be relaxed incorrectly.
- **A full JavaScript parser.**
  It would remove the false writer classification of comments and strings,
  but adds a dependency for little gain.
  The scanner can be replaced later without changing the spec.

### D5. Reader/writer ordering over arrival order

Each call has a position: (message arrival, flow order).
The scheduler keeps, per window, the position of the earliest unfinished reader call
and of the earliest unfinished writer call, counting calls that are queued but not started.
A lane's head call may start when:

- independent: always;
- reader: no unfinished writer call has an earlier position;
- writer: no unfinished reader or writer call has an earlier position.

This makes all mapper-scope accesses equivalent to sequential execution:
a writer runs alone with respect to earlier accesses,
and readers can only overlap with readers.
It cannot deadlock: the unfinished call with the earliest position is always at the head of its lane,
and no rule makes it wait for a later call.

Readers running concurrently matters in practice:
the common pattern is one flow registering entities and many flows looking them up
(for example `custom-measurements.js` reading entity metadata with `context.mapper.get`).

Alternatives:

- **Serialize every flow that touches the mapper scope.**
  Simpler, but lookup-only flows, the most common kind, would lose all parallelism.
- **Wait for the call's turn inside `context.mapper.get` and `set`.**
  The functions are synchronous, so waiting would block the runtime.
  When several flows share a runtime, a later call blocking there
  would prevent an earlier call queued on the same runtime from running: a deadlock.
- **Optimistic execution with re-execution on conflict.**
  Module state, such as a top-level counter, cannot be rolled back,
  so re-running a call would change its results.

### D6. Run-time check of mapper-scope access

The JS context object is already built per call with the flow and step names (`FlowContextHandle::js_context`).
`FlowContext::Mapper` gains the calling flow's identity,
and each `get`, `keys`, `set` and `remove` checks it against a shared table of flow classes.
On a violation, the access completes, a warning is logged once per flow,
and the flow's entry in the table is raised to the stricter class.
The scheduler reads the table before starting each call,
so the correction applies to every call of the flow that has not started.

The access that triggered the warning is not ordered.
This is accepted: it requires a computed property name,
and the warning tells the author which flow to fix.

### D7. globalThis

Runtimes do not share `globalThis`, so flows on different runtimes cannot communicate through it.
ES modules run in strict mode, so a script can only create a global by naming `globalThis`.
A step that names `globalThis` other than to reach a standard built-in
is loaded on the first runtime and classified as writer.
All such flows then share one `globalThis`, and are ordered against each other as today.

### D8. flows.workers = 1 keeps today's code path

With one runtime, the actor calls the existing `MessageProcessor::on_message`.
The pipelined path added 6% to 15% overhead per message with one runtime in the prototype,
so it is only used when there is something to run in parallel.

### D9. Per-runtime interrupt budget

The global `TIME_CREDITS` counter becomes one counter per runtime.
The worker resets it just before running a call.
Previously the counter was reset when a request was sent,
so a queued request could reset the budget of the call that was running.

### D10. Opt-in copies of a single flow

A flow that sets `workers = N` is loaded as `min(N, flows.workers)` copies,
each on a different runtime.
The scheduler treats each copy as a lane, so the rest of the design is unchanged:
per-copy FIFO, at most one call in flight per copy,
output sorted by (arrival, flow order),
and the reader/writer rules applied to calls by position, whatever lane they run on.
Copies of a writer flow therefore still serialize against each other;
copies of reader and independent flows run concurrently.

The setting is opt-in because copies break things only the author can rule out:
module state is split between copies,
read-modify-write of the shared `context.flow` and `context.script` stores can lose updates,
and messages of the flow finish out of order.
No static check can prove a script stateless (any top-level `let` may be a cache or a counter),
so inferring it automatically would be unsound.

Distribution:

- **No `partition`**: a message goes to the first free copy.
  This gives the best throughput for flows that keep no state between messages.
- **`partition = "topic"`**: a message goes to copy `hash(topic) % copies`.
  This keeps per-topic order, and per-topic module state stays correct,
  which covers the common case of a flow keeping per-device or per-series state.
  The hash is stable for a given number of copies.
  Changing the number of copies re-creates the copies, so no state needs to move.
  A hot topic limits its partition to one core; that is the price of per-topic order.

A flow with a periodic step is refused at load time.
`onInterval` would run on one copy and see only that copy's state,
and running it on every copy would multiply its outputs.
Neither is correct, and failing at load time is clearer than either.
`onStartup` runs on every copy, because each copy needs its own initialization.

Alternatives:

- **A per-flow cap below 1 runtime, or a per-flow `parallel = false`.**
  A flow already uses at most one runtime and is already ordered by the reader/writer rules,
  so neither would change anything.
- **Copies that share module state.**
  QuickJS runtimes cannot share JS values, so copies must have separate state.
- **Partition by a payload field or a script-provided key.**
  More flexible, but needs a key expression language or a new script export.
  `topic` covers per-device and per-series state; other keys can be added later
  without breaking the `partition` key.

### D11. Outputs are written only by the actor

JS steps have no filesystem API (`js_lib` provides `console`, `crypto`, the text codecs and the context store).
MQTT and file outputs are written by `FlowsMapper::publish` on the actor task,
after a window's results are sorted, one result at a time,
and each file result is appended and flushed before the next.
So flows writing to the same file need no coordination, and lines keep the single-runtime order.
This is stated as a requirement so that a future JS file API
has to provide the same guarantee, rather than silently losing it.

## Risks / Trade-offs

- [Up to `workers` tokio threads can be blocked by long scripts at once] →
  Document that `flows.workers` should stay at or below the number of cores;
  `auto` never exceeds the number of cores available to the mapper.
- [Worst-case JS heap grows to `workers × flows.memory.heap_size`] →
  Document it next to `flows.memory.heap_size`.
- [Writer flows limit parallelism: one writer call waits for all earlier mapper-scope calls] →
  Writers are rare (registration flows); the ordering is the price of matching sequential results.
  Built-in transformers that only read the context declare `Read`.
- [False writer classification from `mapper` in comments or strings] →
  Costs parallelism only; results are the same as with one runtime.
- [Misclassified access through computed property names is unordered once] →
  Warning names the flow; later calls are ordered.
- [`cache-early-messages` and other `on_context_update` handlers are notified once per window,
  not once per message] →
  Context updates are already asynchronous to these handlers when they come from other actors.
  Revisit if a test shows a dependency on per-message notification.
- [A flow sets `workers` but keeps cross-message state that is not per topic] →
  Results are wrong for that flow only. The flows documentation must state the conditions for `workers`,
  with an example of a counter that breaks.
- [Copies multiply the flow's memory: one module instance and `heap_size` budget per runtime] →
  Bounded by `flows.workers`; documented with the `workers` key.
- [The actor remains serial and caps the process at about 2.5 cores on 4 CPUs] →
  Out of scope; tracked as a follow-up (fewer message clones, one stats update per window).

## Migration Plan

- `flows.workers` defaults to `1`, so upgrading changes nothing.
- Users opt in with `tedge config set flows.workers auto` (one runtime per available core)
  or `tedge config set flows.workers N`, and restart the mapper.
- Rollback: `tedge config unset flows.workers` and restart the mapper.

## Open Questions

- Should the default later become `auto`?
  This depends on system tests passing with several runtimes,
  on the memory cost (`flows.memory.heap_size` applies per runtime),
  and on field experience with the two remaining behavior changes:
  the `globalThis` rule, and `on_context_update` notifications once per pipeline window.
- Should the pipeline window be bounded by time (for example 50 ms) as well as by message count,
  so interval latency under load does not depend on message rate?
