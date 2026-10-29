## ADDED Requirements

### Requirement: Number of JS runtimes is configurable
The `flows.workers` setting SHALL set the number of QuickJS runtimes used by the flows of a mapper.
The setting SHALL accept a positive integer or `auto`.
`auto` SHALL use the number of CPU cores available to the mapper process,
taking into account CPU affinity and cgroup CPU quotas,
and SHALL use one runtime if that number cannot be determined.
The default value SHALL be `1`.
A value of `0` SHALL be rejected by `tedge config set`.
The mapper and `tedge flows test` SHALL both read the setting.

#### Scenario: Default is one runtime
- **WHEN** `flows.workers` is not set
- **THEN** the mapper creates exactly one JS runtime

#### Scenario: Setting more runtimes
- **WHEN** `flows.workers` is set to `4` and the mapper starts
- **THEN** the mapper creates 4 JS runtimes

#### Scenario: One runtime per available core
- **WHEN** `flows.workers` is set to `auto` and the mapper runs in a container limited to 2 CPUs
- **THEN** the mapper creates 2 JS runtimes

#### Scenario: Zero is rejected
- **WHEN** a user runs `tedge config set flows.workers 0`
- **THEN** the command fails and the setting is unchanged

### Requirement: One runtime keeps the current behavior
With `flows.workers = 1`, the mapper SHALL process each MQTT message through all matching flows
before it starts the next message, as it does today.
The order and content of published messages SHALL be identical to the current implementation.

#### Scenario: Same output as before
- **WHEN** the same input messages are processed with `flows.workers = 1`
  by the current implementation and by the new implementation
- **THEN** both publish the same messages in the same order

### Requirement: Each flow is pinned to one runtime
Every JS step of a flow SHALL be loaded into the same runtime.
All `onMessage`, `onInterval` and `onStartup` calls of the flow SHALL run on that runtime.
A new flow SHALL be assigned to the runtime with the fewest flows,
unless the `globalThis` rule in `flows-context-ordering` assigns it to the first runtime.
This requirement applies to flows with one copy; flows with several copies follow
"A flow can opt in to several copies".

#### Scenario: Module state is shared by all calls of a flow
- **WHEN** a script increments a top-level variable in `onMessage`
  and reports it in `onInterval`, with `flows.workers = 4`
- **THEN** `onInterval` reports the number of messages processed by that flow

#### Scenario: Flows are spread across runtimes
- **WHEN** 8 flows with JS steps are loaded with `flows.workers = 4`
- **THEN** each runtime holds 2 of the flows

### Requirement: Flows process MQTT messages concurrently
With `flows.workers > 1`, the mapper SHALL run calls of different flows concurrently,
subject to the ordering rules of `flows-context-ordering`.
The mapper SHALL take further queued MQTT messages as flows complete calls,
without waiting for all flows to finish the current message.
The mapper SHALL take at most 1024 MQTT messages before it handles other queued input
(interval ticks, file-system events, command output).

#### Scenario: Independent flows use several cores
- **WHEN** 4 independent flows with CPU-bound scripts receive messages with `flows.workers = 4` on a 4-core device
- **THEN** the mapper uses more than 2 cores
  and the throughput is more than twice the throughput with `flows.workers = 1`

#### Scenario: A slow flow does not hold back a fast flow
- **WHEN** flow A takes 10 ms per message, flow B takes 0.1 ms per message,
  and both receive a burst of messages
- **THEN** flow B finishes its messages before flow A finishes its messages

#### Scenario: Interval ticks are not starved
- **WHEN** MQTT messages arrive continuously and a flow has an `onInterval` function
- **THEN** `onInterval` is called after at most 1024 further MQTT messages have been taken

### Requirement: Messages of one flow are processed in arrival order
A flow with one copy SHALL process one message at a time, in the order the mapper received the messages.
`onMessage`, `onInterval` and `onStartup` calls of one flow SHALL never overlap.

#### Scenario: Per-flow order
- **WHEN** a flow receives messages m1, m2 and m3, with `flows.workers = 4`
- **THEN** the flow processes m1, then m2, then m3, and starts each one only after the previous one finished

#### Scenario: onInterval does not overlap onMessage
- **WHEN** an interval tick is due while a flow is processing messages
- **THEN** the flow's `onInterval` call starts after its in-progress `onMessage` calls have finished

### Requirement: Output order matches sequential processing
The mapper SHALL publish the outputs of messages taken together
in the order sequential processing would produce:
by input message arrival, then by flow order.

#### Scenario: Same output as one runtime
- **WHEN** the same input messages are fed to 4 stateful flows with `flows.workers = 1` and with `flows.workers = 4`
- **THEN** each flow publishes the same messages, and outputs are published in input order in both runs

### Requirement: Interrupt budget is per runtime
Each runtime SHALL have its own interrupt budget for runaway scripts.
The budget SHALL be reset when a call starts to run, not when the call is queued.
A script that exceeds its budget SHALL be interrupted without affecting calls on other runtimes.

#### Scenario: A runaway script on one runtime
- **WHEN** a script on runtime 1 runs an infinite loop
  while another flow on runtime 2 processes messages
- **THEN** the infinite loop is interrupted with a timeout error,
  and the other flow's calls complete normally

### Requirement: Reloading a script re-creates only its runtime
When a flow's script changes, the mapper SHALL re-create only the runtime that owns the script
and reload the other modules of that runtime.
Flows on other runtimes SHALL keep their module state.

#### Scenario: Module state survives on other runtimes
- **WHEN** flow A on runtime 1 is edited while flow B on runtime 2 counts messages in a top-level variable
- **THEN** flow B's counter keeps its value after the reload

### Requirement: A flow can opt in to several copies
A flow SHALL run as `min(workers, flows.workers)` copies
when its definition sets the top-level `workers` key to a positive integer.
A flow that does not set `workers`, or sets it to `1`, SHALL run as one copy.
Each copy SHALL be loaded on a different runtime, chosen by the fewest-flows rule,
and SHALL have its own module state and its own instances of built-in transformers.
All copies SHALL share the flow's `context.flow` and `context.script` stores.
Each copy SHALL process one message at a time.
Each copy SHALL run `onStartup` when it is loaded.

#### Scenario: Copies of a busy flow
- **WHEN** a flow sets `workers = 4` and `flows.workers = 4`
- **THEN** the flow is loaded on 4 runtimes and processes up to 4 messages at once

#### Scenario: Global setting caps the flow
- **WHEN** a flow sets `workers = 8` and `flows.workers = 2`
- **THEN** the flow runs as 2 copies

#### Scenario: Default is one copy
- **WHEN** a flow does not set `workers`
- **THEN** the flow runs as one copy, whatever the value of `flows.workers`

#### Scenario: Invalid value
- **WHEN** a flow sets `workers = 0`
- **THEN** the flow fails to load, and the error names the flow and the `workers` key

### Requirement: Flows with periodic steps cannot have several copies
The mapper SHALL refuse to load a flow that sets `workers` greater than `1`
and has a periodic step:
a JS step that exports `onInterval`, or a built-in transformer that is periodic.
The error SHALL name the flow and the periodic step.

#### Scenario: onInterval with several copies
- **WHEN** a flow sets `workers = 2` and its script exports `onInterval`
- **THEN** the flow fails to load, and the error says that a flow with `onInterval` cannot set `workers` above 1

### Requirement: Messages of a flow with several copies are distributed by partition
The `partition` key of a flow definition SHALL select how messages are distributed among the flow's copies:
- Without `partition`, each message SHALL go to the first copy that is free, in arrival order.
- With `partition = "topic"`, each message SHALL go to the copy selected by a stable hash of its topic,
  so all messages with the same topic are processed by the same copy, in arrival order.

Any other value of `partition` SHALL make the flow fail to load.
`partition` SHALL be ignored, without an error, for a flow with one copy.

#### Scenario: Same topic, same copy
- **WHEN** a flow sets `workers = 4` and `partition = "topic"`,
  and receives messages m1, m2 and m3 on topic `te/device/a///m/`
- **THEN** one copy processes m1, then m2, then m3

#### Scenario: Per-topic state stays correct
- **WHEN** a flow with `workers = 4` and `partition = "topic"` counts messages per topic in module state
- **THEN** each topic's count equals the number of messages received on that topic

#### Scenario: No partition
- **WHEN** a flow sets `workers = 4` without `partition`, and copies 1 and 2 are busy
- **THEN** the next message goes to copy 3

#### Scenario: Unknown partition
- **WHEN** a flow sets `partition = "payload"`
- **THEN** the flow fails to load, and the error lists `topic` as the supported value

### Requirement: Outputs are written only by the actor
JS steps SHALL NOT have direct access to the filesystem.
MQTT and file outputs of all flows SHALL be written by the flows actor, one result at a time,
in the order defined by "Output order matches sequential processing".
Messages of one result written to a file output SHALL be appended and flushed
before the next result is written.

#### Scenario: Two flows writing to the same file
- **WHEN** two flows with `output.file.path = "/tmp/out.log"` process messages concurrently, with `flows.workers = 4`
- **THEN** no line in the file is interleaved with another,
  and the lines are in the same order as with `flows.workers = 1`
