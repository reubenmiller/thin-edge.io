use crate::access::ContextAccess;
use crate::flow::Flow;
use crate::flow::FlowInput;
use crate::flow::FlowResult;
use crate::flow::Message;
use crate::flow::Partition;
use crate::flow::SourceTag;
use crate::js_lib::kv_store::FlowContextHandle;
use crate::js_runtime::JsRuntime;
use crate::js_runtime::JsRuntimeConfig;
use crate::registry::FlowRegistryExt;
use crate::stats::Counter;
use crate::stats::StatsFilter;
use crate::stats::StatsPublisher;
use crate::LoadError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use futures::StreamExt;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::SystemTime;
use tedge_mqtt_ext::TopicFilter;
use tokio::time::Instant;

pub struct MessageProcessor<Registry> {
    pub registry: Registry,
    pub js_runtime: JsRuntime,
    pub stats: Counter,

    /// The largest number of calls `on_messages` had in flight at once
    #[cfg(test)]
    max_in_flight: usize,
}

impl<Registry: FlowRegistryExt + Send> MessageProcessor<Registry> {
    pub async fn with_default(registry: Registry) -> Result<Self, LoadError> {
        let js_config = JsRuntimeConfig::default();
        let context = FlowContextHandle::default();
        MessageProcessor::with_context(registry, js_config, context).await
    }

    pub async fn with_context(
        registry: Registry,
        js_config: JsRuntimeConfig,
        context: FlowContextHandle,
    ) -> Result<Self, LoadError> {
        let js_runtime = JsRuntime::try_new(js_config, context).await?;
        let stats = Counter::default();

        Ok(MessageProcessor {
            registry,
            js_runtime,
            stats,
            #[cfg(test)]
            max_in_flight: 0,
        })
    }

    pub fn context_handle(&self) -> FlowContextHandle {
        self.js_runtime.context_handle()
    }

    pub async fn load_all_flows(&mut self) {
        self.registry.load_all_flows(&mut self.js_runtime).await;
    }

    pub async fn load_all_flows_from_dir(&mut self, path: &Utf8Path) {
        self.registry
            .load_all_flows_from_dir(path, &mut self.js_runtime)
            .await;
    }

    pub async fn load_single_flow(&mut self, flow: impl AsRef<Utf8Path>) {
        self.registry
            .load_single_flow(&mut self.js_runtime, flow.as_ref())
            .await;
    }

    pub async fn load_single_script(&mut self, script: impl AsRef<Utf8Path>) {
        self.registry
            .load_single_script(&mut self.js_runtime, script.as_ref())
            .await;
    }

    pub fn subscriptions(&self) -> TopicFilter {
        let mut topics = TopicFilter::empty();
        for flow in self.registry.flows() {
            topics.add_all(flow.as_ref().topics())
        }
        topics
    }

    /// Get the next deadline for interval execution across all scripts
    /// Returns None if no scripts have intervals configured
    pub fn next_interval_deadline(&self) -> Option<tokio::time::Instant> {
        self.registry.deadlines().min()
    }

    /// Get the last deadline for interval execution across all scripts Returns
    /// None if no scripts have intervals configured
    ///
    /// This is intended for `tedge flows test` to ensure it processes all
    /// intervals
    pub fn last_interval_deadline(&self) -> Option<tokio::time::Instant> {
        self.registry.deadlines().max()
    }

    pub async fn on_flow_input(
        &mut self,
        flow_path: &Utf8Path,
        timestamp: SystemTime,
        message: &Message,
    ) -> Option<FlowResult> {
        let flow = self.registry.flow_mut(flow_path)?;
        let started_at = self.stats.runtime_on_message_start();
        let flow_output = flow
            .as_mut()
            .on_message(&self.js_runtime, &mut self.stats, timestamp, message)
            .await;
        self.stats.runtime_on_message_done(started_at);
        Some(flow_output)
    }

    pub async fn on_message(
        &mut self,
        timestamp: SystemTime,
        source: &SourceTag,
        message: &Message,
    ) -> Vec<FlowResult> {
        let started_at = self.stats.runtime_on_message_start();

        let mut out_messages = vec![];
        for flow in self.registry.flows_mut() {
            if flow.as_ref().accept_message(source, message) {
                let flow_output = flow
                    .as_mut()
                    .on_message(&self.js_runtime, &mut self.stats, timestamp, message)
                    .await;
                out_messages.push(flow_output);
            }
        }

        self.stats.runtime_on_message_done(started_at);
        out_messages
    }

    /// Process a message, along with the next messages of the same source as long as they are available
    ///
    /// Each copy of a flow processes its messages one at a time, concurrently with the other copies and flows.
    /// The calls of flows sharing the mapper context are ordered by message arrival:
    /// - a call of a flow reading the mapper context starts only once all earlier calls updating it are done
    /// - a call of a flow updating the mapper context starts only once all earlier calls using it are done.
    ///
    /// Each time a call completes, more messages are pulled from `next_message`,
    /// so a busy flow doesn't hold back the others.
    /// At most `max_messages` are processed before returning.
    ///
    /// The results are returned in the order they would have been produced
    /// had the messages been processed one after the other.
    pub async fn on_messages(
        &mut self,
        timestamp: SystemTime,
        _source: &SourceTag,
        message: Message,
        mut next_message: impl FnMut() -> Option<Message>,
        max_messages: usize,
    ) -> Vec<FlowResult> {
        let started_at = self.stats.runtime_on_message_start();
        let context = self.js_runtime.context_handle();

        // Take the copies out of their flows, so all the copies can be processed concurrently
        let mut copies: HashMap<Utf8PathBuf, Vec<Flow>> = self
            .registry
            .flows_mut()
            .map(|flow| {
                let flow = flow.as_mut();
                (flow.source.clone(), std::mem::take(&mut flow.copies))
            })
            .collect();
        // The flows are borrowed by the copies being processed until the end of this block
        let (mut results, count) = {
            let mut flow_copies: HashMap<&Utf8PathBuf, &mut Vec<Flow>> =
                copies.iter_mut().collect();
            let mut flows: Vec<FlowLanes> = self
                .registry
                .flows_mut()
                .enumerate()
                .map(|(rank, flow)| {
                    let flow = flow.as_mut();
                    let copies = flow_copies.remove(&flow.source).into_iter().flatten();
                    FlowLanes::new(rank, &context, flow, copies)
                })
                .collect();

            let js_runtime = &self.js_runtime;
            let mut pending = PendingCalls::default();
            let mut generation = context.access_generation();
            let mut in_flight = futures::stream::FuturesUnordered::new();
            let mut results = vec![];
            let mut next = Some(message);
            let mut count = 0;
            loop {
                while let Some(message) = next.take() {
                    count += 1;
                    let message = Arc::new(message);
                    for lanes in flows.iter_mut() {
                        lanes.enqueue(count, &message, &mut pending);
                    }
                    if count < max_messages {
                        next = next_message();
                    }
                }

                // Apply the classes raised by accesses to the mapper context the flow sources don't show
                let current_generation = context.access_generation();
                if current_generation != generation {
                    generation = current_generation;
                    for lanes in flows.iter_mut() {
                        lanes.reclassify(&context, &mut pending);
                    }
                }

                for (index, lanes) in flows.iter_mut().enumerate() {
                    let rank = lanes.rank;
                    while let Some((copy, call)) = lanes.next_call(&pending) {
                        let Some((flow, mut stats)) = lanes.copies[copy].take() else {
                            unreachable!("next_call only returns free copies");
                        };
                        in_flight.push(async move {
                            let result = flow
                                .on_message(js_runtime, &mut stats, timestamp, &call.message)
                                .await;
                            (index, copy, rank, flow, stats, call, result)
                        });
                    }
                }
                #[cfg(test)]
                {
                    self.max_in_flight = self.max_in_flight.max(in_flight.len());
                }

                let Some((index, copy, rank, flow, stats, call, result)) = in_flight.next().await
                else {
                    break;
                };
                flows[index].copies[copy] = Some((flow, stats));
                pending.unregister(call.access, (call.arrival, rank));
                results.push((call.arrival, rank, result));

                if count < max_messages {
                    next = next_message();
                }
            }

            for lanes in flows {
                for (_, stats) in lanes.copies.into_iter().flatten() {
                    self.stats.merge(stats);
                }
            }
            (results, count)
        };
        for flow in self.registry.flows_mut() {
            let flow = flow.as_mut();
            if let Some(copies) = copies.remove(&flow.source) {
                flow.copies = copies;
            }
        }
        for _ in 1..count {
            self.stats.runtime_on_message_start();
        }
        for _ in 0..count {
            self.stats.runtime_on_message_done(started_at);
        }
        results.sort_by_key(|(arrival, rank, _)| (*arrival, *rank));
        results.into_iter().map(|(_, _, result)| result).collect()
    }

    pub async fn on_startup(&mut self, timestamp: SystemTime) -> Vec<FlowResult> {
        let mut out_messages = vec![];

        for flow in self.registry.flows_mut() {
            // Each copy of a flow has its own module state, hence has to be started too
            let flow = flow.as_mut();
            if flow.steps.iter().any(|s| s.should_execute_startup()) {
                let flow_output = flow
                    .on_startup(&self.js_runtime, &mut self.stats, timestamp)
                    .await;
                out_messages.push(flow_output);
            }
            for copy in flow.copies.iter_mut() {
                if copy.steps.iter().any(|s| s.should_execute_startup()) {
                    let flow_output = copy
                        .on_startup(&self.js_runtime, &mut self.stats, timestamp)
                        .await;
                    out_messages.push(flow_output);
                }
            }
        }

        out_messages
    }

    pub async fn on_interval(&mut self, timestamp: SystemTime, now: Instant) -> Vec<FlowResult> {
        let mut out_messages = vec![];
        for flow in self.registry.flows_mut() {
            let flow_output = flow
                .as_mut()
                .on_interval(&self.js_runtime, &mut self.stats, timestamp, now)
                .await;
            out_messages.push(flow_output);
        }
        out_messages
    }

    pub async fn on_context_update(&mut self, timestamp: SystemTime) -> Vec<FlowResult> {
        let mut out_messages = vec![];
        for update in self.context_handle().drain_updates() {
            for flow in self.registry.flows_mut() {
                let flow = flow.as_mut();
                let flow_output = flow
                    .on_context_update(&self.js_runtime, &mut self.stats, timestamp, &update)
                    .await;
                out_messages.extend(flow_output);
                for copy in flow.copies.iter_mut() {
                    let flow_output = copy
                        .on_context_update(&self.js_runtime, &mut self.stats, timestamp, &update)
                        .await;
                    out_messages.extend(flow_output);
                }
            }
        }
        out_messages
    }

    pub async fn dump_processing_stats<P: StatsPublisher>(
        &self,
        publisher: &P,
        filter: &StatsFilter,
    ) -> Vec<P::Record> {
        self.stats.dump_processing_stats(publisher, filter)
    }

    pub async fn dump_memory_stats<P: StatsPublisher>(&self, publisher: &P) -> Option<P::Record> {
        let stats = self.js_runtime.dump_memory_stats().await;
        publisher.publish_record(&"memory", stats)
    }

    pub async fn reload_script(&mut self, path: &Utf8Path) -> Vec<Utf8PathBuf> {
        self.registry
            .reload_script(&mut self.js_runtime, path)
            .await
    }

    pub async fn remove_script(&mut self, path: &Utf8Path) {
        self.registry.remove_script(path).await;
    }

    pub async fn add_flow(&mut self, path: &Utf8Path) {
        self.registry.add_flow(&mut self.js_runtime, path).await;
    }

    pub async fn remove_flow(&mut self, path: &Utf8Path) {
        self.registry.remove_flow(path).await;
        self.js_runtime.release_flow(path);
    }
}

/// The copies of a flow, each processing one message at a time
struct FlowLanes<'a> {
    /// The rank of the flow in the registry, used to order the results
    rank: usize,
    /// The flow path, as known by the mapper context
    source: String,
    /// How the flow uses the mapper context, as declared by its steps
    declared: ContextAccess,
    /// How the flow uses the mapper context, possibly raised after a misclassified access
    access: ContextAccess,
    input: Vec<FlowInput>,
    partition: Partition,
    /// Each copy with its stats, unless a message is being processed by this copy
    copies: Vec<Option<(&'a mut Flow, Counter)>>,
    /// The messages to be processed: one queue shared by all the copies,
    /// or one queue per copy when messages are partitioned by topic
    queues: Vec<VecDeque<Call>>,
}

/// A message to be processed by a flow
struct Call {
    arrival: usize,
    message: Arc<Message>,
    /// The class under which the call has been registered among the pending calls
    access: ContextAccess,
}

impl<'a> FlowLanes<'a> {
    fn new(
        rank: usize,
        context: &FlowContextHandle,
        flow: &'a mut Flow,
        copies: impl Iterator<Item = &'a mut Flow>,
    ) -> Self {
        let source = flow.source.to_string();
        let declared = flow.context_access();
        let access = context.flow_access(&source, declared);
        let input = flow.input.clone();
        let partition = flow.partition;
        let copies: Vec<_> = std::iter::once(flow)
            .chain(copies)
            .map(|flow| Some((flow, Counter::default())))
            .collect();
        let queue_count = match partition {
            Partition::Topic => copies.len(),
            Partition::None => 1,
        };
        FlowLanes {
            rank,
            source,
            declared,
            access,
            input,
            partition,
            copies,
            queues: (0..queue_count).map(|_| VecDeque::new()).collect(),
        }
    }

    /// Queue a message, if accepted by this flow
    fn enqueue(&mut self, arrival: usize, message: &Arc<Message>, pending: &mut PendingCalls) {
        if !self.input.iter().any(|input| input.accept_message(message)) {
            return;
        }
        let queue = match self.partition {
            Partition::Topic => topic_hash(&message.topic) % self.queues.len(),
            Partition::None => 0,
        };
        pending.register(self.access, (arrival, self.rank));
        self.queues[queue].push_back(Call {
            arrival,
            message: message.clone(),
            access: self.access,
        });
    }

    /// Apply to the queued calls the class of the flow, if raised since they were queued
    fn reclassify(&mut self, context: &FlowContextHandle, pending: &mut PendingCalls) {
        let access = context.flow_access(&self.source, self.declared);
        if access == self.access {
            return;
        }
        self.access = access;
        for call in self.queues.iter_mut().flatten() {
            pending.unregister(call.access, (call.arrival, self.rank));
            pending.register(access, (call.arrival, self.rank));
            call.access = access;
        }
    }

    /// Take the next call that a free copy of this flow can start, along with the copy index
    fn next_call(&mut self, pending: &PendingCalls) -> Option<(usize, Call)> {
        let rank = self.rank;
        let startable = |queue: &VecDeque<Call>| {
            queue
                .front()
                .is_some_and(|call| pending.allows(call.access, (call.arrival, rank)))
        };
        match self.partition {
            Partition::None => {
                let copy = self.copies.iter().position(Option::is_some)?;
                if !startable(&self.queues[0]) {
                    return None;
                }
                Some((copy, self.queues[0].pop_front()?))
            }
            Partition::Topic => {
                let copy = (0..self.copies.len())
                    .find(|copy| self.copies[*copy].is_some() && startable(&self.queues[*copy]))?;
                Some((copy, self.queues[copy].pop_front()?))
            }
        }
    }
}

/// The position of a call: the arrival of the message, then the rank of the flow
type Position = (usize, usize);

/// The calls using the mapper context that are queued or in flight
#[derive(Default)]
struct PendingCalls {
    readers: BTreeSet<Position>,
    writers: BTreeSet<Position>,
}

impl PendingCalls {
    fn register(&mut self, access: ContextAccess, position: Position) {
        match access {
            ContextAccess::Independent => {}
            ContextAccess::Read => {
                self.readers.insert(position);
            }
            ContextAccess::Write => {
                self.writers.insert(position);
            }
        }
    }

    fn unregister(&mut self, access: ContextAccess, position: Position) {
        match access {
            ContextAccess::Independent => {}
            ContextAccess::Read => {
                self.readers.remove(&position);
            }
            ContextAccess::Write => {
                self.writers.remove(&position);
            }
        }
    }

    /// Check that a call can start without breaking the arrival order of mapper context accesses
    fn allows(&self, access: ContextAccess, position: Position) -> bool {
        let earlier =
            |calls: &BTreeSet<Position>| calls.first().is_some_and(|first| *first < position);
        match access {
            ContextAccess::Independent => true,
            ContextAccess::Read => !earlier(&self.writers),
            ContextAccess::Write => !earlier(&self.writers) && !earlier(&self.readers),
        }
    }
}

/// A hash of a topic, stable for a given build, used to partition messages among flow copies
fn topic_hash(topic: &str) -> usize {
    use std::hash::Hash;
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    topic.hash(&mut hasher);
    hasher.finish() as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::BaseFlowRegistry;
    use crate::registry::FlowRegistryExt;
    use std::collections::BTreeMap;

    const COUNTER: &str = r#"
        const utf8 = new TextDecoder();
        let seq = 0;
        export function onMessage(message) {
            seq++;
            return [{ topic: `out/${message.topic}`, payload: `${seq}:${utf8.decode(message.payload)}` }];
        }
    "#;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_flows_produce_the_same_outputs_as_sequential_flows() {
        let messages: Vec<Message> = (0..200)
            .map(|i| Message::new(format!("test/{}", i % 5), format!("{i}")))
            .collect();

        let sequential = process_all(1, &messages).await;
        let concurrent = process_all(4, &messages).await;

        assert_eq!(sequential.len(), 4);
        assert_eq!(sequential, concurrent);
    }

    /// Process all the messages in one go, returning the outputs of each flow
    async fn process_all(workers: usize, messages: &[Message]) -> BTreeMap<String, Vec<String>> {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(dir_path.join("counter.js"), COUNTER).unwrap();
        for i in 0..4 {
            // Every flow also accepts the messages published on test/4
            let config = format!(
                r#"
                input.mqtt.topics = ["test/{i}", "test/4"]
                steps = [{{ script = "counter.js" }}]
                "#
            );
            std::fs::write(dir_path.join(format!("flow{i}.toml")), config).unwrap();
        }
        let registry = BaseFlowRegistry::new(std::collections::HashMap::new(), dir_path).unwrap();
        let config = JsRuntimeConfig {
            workers,
            ..JsRuntimeConfig::default()
        };
        let mut processor =
            MessageProcessor::with_context(registry, config, FlowContextHandle::default())
                .await
                .unwrap();
        processor.load_all_flows().await;

        let mut input = messages.iter().cloned();
        let first = input.next().unwrap();
        let results = processor
            .on_messages(
                SystemTime::now(),
                &SourceTag::Mqtt,
                first,
                || input.next(),
                usize::MAX,
            )
            .await;

        let mut outputs: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut last_input = 0;
        for result in results {
            let FlowResult::Ok { flow, messages, .. } = result else {
                let FlowResult::Err { error, .. } = result else {
                    unreachable!()
                };
                panic!("Unexpected flow error: {error}");
            };
            for message in messages {
                let payload = message.payload_str().unwrap().to_string();
                let input: usize = payload.split(':').nth(1).unwrap().parse().unwrap();
                assert!(
                    input >= last_input,
                    "Results must be returned in input order"
                );
                last_input = input;
                let flow = flow.file_name().unwrap().to_string();
                outputs.entry(flow).or_default().push(payload);
            }
        }
        outputs
    }

    /// Flows loaded from a temporary directory, processing messages as the actor does
    struct TestFlows {
        dir: tempfile::TempDir,
        workers: usize,
        processor: MessageProcessor<BaseFlowRegistry>,
    }

    impl TestFlows {
        async fn new(workers: usize, files: &[(&str, &str)]) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let dir_path = Utf8Path::from_path(dir.path()).unwrap();
            for (name, content) in files {
                std::fs::write(dir_path.join(name), content).unwrap();
            }
            let registry =
                BaseFlowRegistry::new(std::collections::HashMap::new(), dir_path).unwrap();
            let config = JsRuntimeConfig {
                workers,
                ..JsRuntimeConfig::default()
            };
            let mut processor =
                MessageProcessor::with_context(registry, config, FlowContextHandle::default())
                    .await
                    .unwrap();
            processor.load_all_flows().await;
            TestFlows {
                dir,
                workers,
                processor,
            }
        }

        fn path(&self, name: &str) -> Utf8PathBuf {
            Utf8Path::from_path(self.dir.path())
                .unwrap()
                .canonicalize_utf8()
                .unwrap()
                .join(name)
        }

        /// Process the messages, returning the output payloads tagged by flow file name
        async fn process(&mut self, messages: &[Message]) -> Vec<(String, String)> {
            let timestamp = SystemTime::now();
            let results = if self.workers == 1 {
                let mut results = vec![];
                for message in messages {
                    results.extend(
                        self.processor
                            .on_message(timestamp, &SourceTag::Mqtt, message)
                            .await,
                    );
                }
                results
            } else {
                let mut input = messages.iter().cloned();
                let first = input.next().unwrap();
                self.processor
                    .on_messages(
                        timestamp,
                        &SourceTag::Mqtt,
                        first,
                        || input.next(),
                        usize::MAX,
                    )
                    .await
            };
            let mut outputs = vec![];
            for result in results {
                match result {
                    FlowResult::Ok { flow, messages, .. } => {
                        let flow = flow.file_name().unwrap().to_string();
                        for message in messages {
                            outputs.push((flow.clone(), message.payload_str().unwrap().into()));
                        }
                    }
                    FlowResult::Err { flow, error, .. } => {
                        let flow = flow.file_name().unwrap().to_string();
                        outputs.push((flow, format!("error: {error}")));
                    }
                }
            }
            outputs
        }

        fn mapper_value(&self, key: &str) -> serde_json::Value {
            self.processor.context_handle().get_value(key).into()
        }
    }

    fn flow_toml(topic: &str, script: &str, extra: &str) -> String {
        format!(
            r#"
            {extra}
            input.mqtt.topics = ["{topic}"]
            steps = [{{ script = "{script}" }}]
            "#
        )
    }

    /// A CPU-bound loop of the given number of iterations
    fn busy(work: usize) -> String {
        format!("let x = 0; for (let i = 0; i < {work}; i++) {{ x += i % 7; }}")
    }

    fn payloads(outputs: &[(String, String)], flow: &str) -> Vec<String> {
        outputs
            .iter()
            .filter(|(f, _)| f == flow)
            .map(|(_, p)| p.clone())
            .collect()
    }

    /// A writer flow stores each message index; a reader flow reads it back for the next message
    async fn reader_reads_writer_values(
        workers: usize,
        writer_work: usize,
        reader_work: usize,
    ) -> Vec<String> {
        let writer = format!(
            r#"
            const utf8 = new TextDecoder();
            export function onMessage(message, context) {{
                {}
                context.mapper.set("last", utf8.decode(message.payload));
                return [];
            }}"#,
            busy(writer_work)
        );
        let reader = format!(
            r#"
            export function onMessage(message, context) {{
                {}
                return [{{ topic: "out/reader", payload: `${{context.mapper.get("last")}}` }}];
            }}"#,
            busy(reader_work)
        );
        let mut flows = TestFlows::new(
            workers,
            &[
                ("writer.js", &writer),
                ("reader.js", &reader),
                ("writer.toml", &flow_toml("w", "writer.js", "")),
                ("reader.toml", &flow_toml("r", "reader.js", "")),
            ],
        )
        .await;
        let messages: Vec<_> = (0..500)
            .flat_map(|i| [Message::new("w", format!("{i}")), Message::new("r", "")])
            .collect();
        payloads(&flows.process(&messages).await, "reader.toml")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_reader_sees_the_value_written_for_the_earlier_message() {
        let expected: Vec<String> = (0..500).map(|i| i.to_string()).collect();
        // A slow writer
        assert_eq!(reader_reads_writer_values(1, 2000, 0).await, expected);
        assert_eq!(reader_reads_writer_values(4, 2000, 0).await, expected);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_reader_never_sees_a_value_written_for_a_later_message() {
        let expected: Vec<String> = (0..500).map(|i| i.to_string()).collect();
        // A fast writer and a slow reader
        assert_eq!(reader_reads_writer_values(1, 0, 2000).await, expected);
        assert_eq!(reader_reads_writer_values(4, 0, 2000).await, expected);
    }

    const INCREMENT: &str = r#"
        export function onMessage(message, context) {
            const count = context.mapper.get("count") || 0;
            context.mapper.set("count", count + 1);
            return [];
        }
    "#;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn two_writers_never_lose_updates() {
        let mut flows = TestFlows::new(
            4,
            &[
                ("increment.js", INCREMENT),
                ("a.toml", &flow_toml("a", "increment.js", "")),
                ("b.toml", &flow_toml("b", "increment.js", "")),
            ],
        )
        .await;
        let messages: Vec<_> = (0..500)
            .flat_map(|_| [Message::new("a", ""), Message::new("b", "")])
            .collect();
        flows.process(&messages).await;
        assert_eq!(flows.mapper_value("count"), serde_json::json!(1000));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn copies_of_a_writer_never_lose_updates() {
        let mut flows = TestFlows::new(
            4,
            &[
                ("increment.js", INCREMENT),
                ("a.toml", &flow_toml("a", "increment.js", "workers = 4")),
            ],
        )
        .await;
        assert_eq!(
            flows
                .processor
                .registry
                .flow(&flows.path("a.toml"))
                .unwrap()
                .copies
                .len(),
            3
        );
        let messages: Vec<_> = (0..1000).map(|_| Message::new("a", "")).collect();
        flows.process(&messages).await;
        assert_eq!(flows.mapper_value("count"), serde_json::json!(1000));
    }

    /// A script reading the mapper context
    const READER: &str = r#"
        export function onMessage(message, context) {
            const entity = context.mapper.get(message.topic);
            return [{ topic: "out/read", payload: message.topic }];
        }
    "#;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn readers_run_concurrently() {
        let mut files = vec![("reader.js".to_string(), READER.to_string())];
        for i in 0..4 {
            files.push((
                format!("reader{i}.toml"),
                flow_toml(&format!("t/{i}"), "reader.js", ""),
            ));
        }
        let files: Vec<_> = files
            .iter()
            .map(|(n, c)| (n.as_str(), c.as_str()))
            .collect();
        let mut flows = TestFlows::new(4, &files).await;
        let messages: Vec<_> = (0..8)
            .map(|i| Message::new(format!("t/{}", i % 4), ""))
            .collect();
        let outputs = flows.process(&messages).await;
        assert_eq!(outputs.len(), 8);
        assert_eq!(flows.processor.max_in_flight, 4);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn copies_of_a_flow_run_concurrently() {
        let mut flows = TestFlows::new(
            4,
            &[
                ("reader.js", READER),
                ("reader.toml", &flow_toml("t/+", "reader.js", "workers = 4")),
            ],
        )
        .await;
        let messages: Vec<_> = (0..8).map(|i| Message::new(format!("t/{i}"), "")).collect();
        let outputs = flows.process(&messages).await;
        assert_eq!(outputs.len(), 8);
        assert_eq!(flows.processor.max_in_flight, 4);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn copies_partitioned_by_topic_keep_per_topic_order_and_state() {
        let script = r#"
            const utf8 = new TextDecoder();
            const counts = {};
            export function onMessage(message) {
                counts[message.topic] = (counts[message.topic] || 0) + 1;
                return [{ topic: "out", payload: `${message.topic}:${counts[message.topic]}:${utf8.decode(message.payload)}` }];
            }
        "#;
        let mut flows = TestFlows::new(
            4,
            &[
                ("count.js", script),
                (
                    "count.toml",
                    &flow_toml(
                        "t/+",
                        "count.js",
                        r#"workers = 4
                    partition = "topic""#,
                    ),
                ),
            ],
        )
        .await;
        let messages: Vec<_> = (0..400)
            .map(|i| Message::new(format!("t/{}", i % 8), format!("{i}")))
            .collect();
        let outputs = flows.process(&messages).await;
        assert_eq!(outputs.len(), 400);
        let mut seen: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (_, payload) in outputs {
            let mut parts = payload.split(':');
            let topic = parts.next().unwrap().to_string();
            let count: usize = parts.next().unwrap().parse().unwrap();
            let input: usize = parts.next().unwrap().parse().unwrap();
            let inputs = seen.entry(topic).or_default();
            assert_eq!(count, inputs.len() + 1, "per-topic state must be kept");
            assert!(
                inputs.last().is_none_or(|last| *last < input),
                "per-topic order must be kept"
            );
            inputs.push(input);
        }
        assert_eq!(seen.len(), 8);
    }

    #[tokio::test]
    async fn the_number_of_copies_is_capped_and_validated() {
        let flows = TestFlows::new(
            2,
            &[
                ("echo.js", "export function onMessage(m) { return [m]; }"),
                ("capped.toml", &flow_toml("a", "echo.js", "workers = 8")),
                ("zero.toml", &flow_toml("b", "echo.js", "workers = 0")),
                ("payload.toml", &flow_toml("c", "echo.js", r#"partition = "payload""#)),
                (
                    "periodic.js",
                    "export function onMessage(m) { return [m]; }\nexport function onInterval() { return []; }",
                ),
                ("periodic.toml", &flow_toml("d", "periodic.js", "workers = 2")),
            ],
        )
        .await;
        let registry = &flows.processor.registry;
        assert_eq!(
            registry
                .flow(&flows.path("capped.toml"))
                .unwrap()
                .copies
                .len(),
            1
        );
        for broken in ["zero.toml", "payload.toml", "periodic.toml"] {
            assert!(registry.flow(&flows.path(broken)).is_none(), "{broken}");
            assert!(
                registry.unloaded().contains(&flows.path(broken)),
                "{broken}"
            );
        }
    }

    #[tokio::test]
    async fn the_number_of_copies_is_one_by_default() {
        let flows = TestFlows::new(
            4,
            &[
                ("echo.js", "export function onMessage(m) { return [m]; }"),
                ("echo.toml", &flow_toml("a", "echo.js", "")),
            ],
        )
        .await;
        assert!(flows
            .processor
            .registry
            .flow(&flows.path("echo.toml"))
            .unwrap()
            .copies
            .is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn built_in_flows_keep_their_order() {
        let reader = r#"
            export function onMessage(message, context) {
                const units = context.mapper.get("units/temperature");
                return [{ topic: "out", payload: JSON.stringify(units) }];
            }
        "#;
        let mut flows = TestFlows::new(
            4,
            &[
                ("reader.js", reader),
                (
                    "units.toml",
                    r#"
                    input.mqtt.topics = ["units/#"]
                    steps = [{ builtin = "update-context", config.topics = ["units/#"] }]
                    "#,
                ),
                ("measurements.toml", &flow_toml("m", "reader.js", "")),
            ],
        )
        .await;
        let messages: Vec<_> = (0..200)
            .flat_map(|i| {
                [
                    Message::new("units/temperature", format!(r#"{{"unit":"{i}"}}"#)),
                    Message::new("m", ""),
                ]
            })
            .collect();
        let outputs = payloads(&flows.process(&messages).await, "measurements.toml");
        let expected: Vec<String> = (0..200).map(|i| format!(r#"{{"unit":"{i}"}}"#)).collect();
        assert_eq!(outputs, expected);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_misclassified_access_raises_the_class_of_the_flow() {
        let sneaky = r#"
            export function onMessage(message, context) {
                const scope = "map" + "per";
                const count = context[scope].get("count") || 0;
                context[scope].set("count", count + 1);
                return [];
            }
        "#;
        let mut flows = TestFlows::new(
            4,
            &[
                ("sneaky.js", sneaky),
                ("sneaky.toml", &flow_toml("s", "sneaky.js", "")),
            ],
        )
        .await;
        let source = flows.path("sneaky.toml").to_string();
        let context = flows.processor.context_handle();
        assert_eq!(
            context.flow_access(&source, ContextAccess::Independent),
            ContextAccess::Independent
        );
        let generation = context.access_generation();

        flows.process(&[Message::new("s", "")]).await;

        assert_eq!(
            context.flow_access(&source, ContextAccess::Independent),
            ContextAccess::Write
        );
        assert_ne!(context.access_generation(), generation);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_runaway_script_does_not_block_flows_on_other_runtimes() {
        let mut flows = TestFlows::new(
            2,
            &[
                ("loop.js", "export function onMessage(m) { while(true); }"),
                (
                    "echo.js",
                    "export function onMessage(m) { return [{ topic: 'out', payload: 'ok' }]; }",
                ),
                ("loop.toml", &flow_toml("l", "loop.js", "")),
                ("echo.toml", &flow_toml("e", "echo.js", "")),
            ],
        )
        .await;
        let mut messages = vec![Message::new("l", "")];
        messages.extend((0..50).map(|_| Message::new("e", "")));
        let outputs = flows.process(&messages).await;

        let looping = payloads(&outputs, "loop.toml");
        assert_eq!(looping.len(), 1);
        assert!(looping[0].contains("interrupted"), "{looping:?}");
        assert_eq!(payloads(&outputs, "echo.toml"), vec!["ok"; 50]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn reloading_a_script_keeps_the_state_of_flows_on_other_runtimes() {
        let counter = r#"
            let count = 0;
            export function onMessage(m) { count++; return [{ topic: "out", payload: `${count}` }]; }
        "#;
        let mut flows = TestFlows::new(
            2,
            &[
                ("a.js", counter),
                ("b.js", counter),
                ("a.toml", &flow_toml("a", "a.js", "")),
                ("b.toml", &flow_toml("b", "b.js", "")),
            ],
        )
        .await;
        let messages = [Message::new("a", ""), Message::new("b", "")];
        flows.process(&messages).await;
        flows.process(&messages).await;

        std::fs::write(flows.path("a.js"), counter).unwrap();
        flows.processor.reload_script(&flows.path("a.js")).await;

        let outputs = flows.process(&messages).await;
        assert_eq!(payloads(&outputs, "a.toml"), vec!["1"], "a.js is reloaded");
        assert_eq!(
            payloads(&outputs, "b.toml"),
            vec!["3"],
            "b.js keeps its state"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn flows_sharing_an_output_file_produce_the_same_results_with_one_or_more_runtimes() {
        let script = r#"
            const utf8 = new TextDecoder();
            export function onMessage(m) { return [{ topic: m.topic, payload: utf8.decode(m.payload) }]; }
        "#;
        let file_flow = |topic: &str| {
            format!(
                r#"
                input.mqtt.topics = ["{topic}"]
                steps = [{{ script = "echo.js" }}]
                output.file.path = "/tmp/shared.log"
                "#
            )
        };
        let files = [
            ("echo.js", script.to_string()),
            ("a.toml", file_flow("a")),
            ("b.toml", file_flow("b")),
        ];
        let files: Vec<_> = files.iter().map(|(n, c)| (*n, c.as_str())).collect();
        let messages: Vec<_> = (0..200)
            .map(|i| Message::new(if i % 3 == 0 { "a" } else { "b" }, format!("{i}")))
            .collect();

        let sequential = TestFlows::new(1, &files).await.process(&messages).await;
        let concurrent = TestFlows::new(4, &files).await.process(&messages).await;
        assert_eq!(sequential.len(), 200);
        assert_eq!(sequential, concurrent);
    }
}
