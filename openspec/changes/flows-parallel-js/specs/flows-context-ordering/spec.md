## ADDED Requirements

### Requirement: Each JS step is classified by its use of context.mapper
When a JS step is loaded, the mapper SHALL classify it from its source text:
- **Independent** when the source does not contain the identifier `mapper`.
- **Reader** when every occurrence of the identifier `mapper` is immediately followed,
  ignoring whitespace, by `.get(` or `.keys(`.
- **Writer** in every other case,
  including `mapper.set(`, `mapper.remove(`, aliasing (`const m = context.mapper`),
  destructuring (`const { mapper } = context`), and occurrences in comments or strings.

The classification SHALL be recomputed when the script is reloaded.

#### Scenario: Script that never uses the mapper scope
- **WHEN** a script only uses `context.flow` and `context.script`
- **THEN** the step is classified as independent

#### Scenario: Script that only reads the mapper scope
- **WHEN** a script calls `context.mapper.get(id)` and `context.mapper.keys()` and nothing else on `mapper`
- **THEN** the step is classified as reader

#### Scenario: Script that writes the mapper scope
- **WHEN** a script calls `context.mapper.set(id, value)`
- **THEN** the step is classified as writer

#### Scenario: Aliased access is treated as a write
- **WHEN** a script contains `const { mapper } = context` and only calls `mapper.get(id)`
- **THEN** the step is classified as writer

### Requirement: Built-in transformers declare their context access
Each built-in transformer SHALL declare whether it is independent, reader or writer of the mapper scope.
A transformer that does not declare it SHALL be treated as writer.

#### Scenario: Undeclared transformer
- **WHEN** a transformer registered by another crate does not declare its context access
- **THEN** every step using it is classified as writer

#### Scenario: Transformer that does not touch the context
- **WHEN** a flow uses only `add-timestamp` and `set-topic`
- **THEN** the flow is classified as independent

### Requirement: A flow's class is the strongest class of its steps
A flow SHALL be classified as writer if any of its steps is a writer,
otherwise as reader if any of its steps is a reader,
otherwise as independent.

#### Scenario: Mixed flow
- **WHEN** a flow has an independent JS step followed by a reader JS step
- **THEN** the flow is classified as reader

### Requirement: Mapper scope accesses happen in arrival order
With `flows.workers > 1`, calls of flows SHALL be ordered by (message arrival, flow order),
and the mapper SHALL start them under these rules:
- A call of an independent flow SHALL start as soon as the copy it is assigned to is free.
- A call of a reader flow SHALL also wait until every earlier call of a writer flow has finished.
- A call of a writer flow SHALL also wait until every earlier call of a reader or writer flow has finished.

"Earlier" SHALL include calls that are queued but not yet started.
Calls made by different copies of the same flow SHALL follow the same rules as calls of different flows.
Every `context.mapper` value read or written by a correctly classified flow
SHALL then be the same as with `flows.workers = 1`.
This guarantee does not cover writes made by other components outside the flows actor,
such as entity registrations stored by the Cumulocity mapper;
those are not ordered against flows today either.

#### Scenario: A reader sees the write for an earlier message
- **WHEN** a writer flow stores each message index in `context.mapper`,
  and a reader flow reads it for the next message, with `flows.workers = 4`
- **THEN** every read returns the value that the same run with `flows.workers = 1` returns

#### Scenario: A reader never sees a write for a later message
- **WHEN** a writer flow is faster than a reader flow, and both receive the same messages
- **THEN** no read by the reader flow returns a value written for a later message

#### Scenario: No lost updates
- **WHEN** two writer flows each increment one `context.mapper` counter 500 times, with `flows.workers = 4`
- **THEN** the counter ends at 1000

#### Scenario: Copies of a writer flow keep mapper order
- **WHEN** a writer flow with `workers = 4` increments one `context.mapper` counter for each of 1000 messages
- **THEN** the counter ends at 1000

#### Scenario: Readers run in parallel
- **WHEN** 4 reader flows with CPU-bound scripts receive messages and no writer flow is loaded
- **THEN** their calls run concurrently, as for independent flows

#### Scenario: Built-in flows keep their order
- **WHEN** the `units` flow stores units for a message and the `measurements` flow reads them for a later message
- **THEN** the `measurements` flow uses the units stored for the earlier message

### Requirement: Misclassified access is detected and corrected
Each `context.mapper` access SHALL be checked against the class of the calling flow.
When an independent flow reads, or a reader or independent flow writes, the mapper SHALL:
- complete the access,
- log a warning once per flow, naming the flow and the class it is moved to,
- apply the stricter class to every call of the flow that has not started yet.

The class SHALL stay corrected until the flow is reloaded.

#### Scenario: Computed property name
- **WHEN** a script classified as independent calls `context["map" + "per"].set("k", 1)`
- **THEN** the mapper logs a warning naming the flow,
  and the flow's later calls are ordered as a writer

### Requirement: Flows that share globalThis stay together
The mapper SHALL load a flow on the first runtime and classify it as writer
when one of its JS steps contains `globalThis`
other than as `globalThis.<name>` where `<name>` is a standard built-in of the runtime
(for example `globalThis.TextDecoder` or `globalThis.JSON`).

#### Scenario: Two flows sharing state through globalThis
- **WHEN** flow A sets `globalThis.lastSeen` and flow B reads it, with `flows.workers = 4`
- **THEN** both flows run on the first runtime, and flow B sees the value set by flow A for the earlier message

#### Scenario: Reading a built-in through globalThis
- **WHEN** a script only contains `new globalThis.TextDecoder()`
- **THEN** its flow is assigned to a runtime by the normal rule
