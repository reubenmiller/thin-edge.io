use crate::access::SourceAnalysis;
use crate::js_lib;
use crate::js_lib::kv_store::CallingFlow;
use crate::js_lib::kv_store::FlowContextHandle;
use crate::js_script::JsScript;
use crate::js_value::JsonValue;
use crate::LoadError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use rquickjs::module::Evaluated;
use rquickjs::Ctx;
use rquickjs::Error;
use rquickjs::Module;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::debug;
use tracing::Instrument;

pub struct JsRuntime {
    workers: Vec<JsWorkerHandle>,
    store: FlowContextHandle,
    /// The worker assigned to each copy of a flow: all the steps of a copy are run by the same worker
    flow_workers: HashMap<(Utf8PathBuf, usize), usize>,
    /// The worker running each module, along with the module flow and source
    module_sources: HashMap<String, ModuleSource>,
    config: JsRuntimeConfig,
}

struct ModuleSource {
    worker: usize,
    flow: String,
    source: Vec<u8>,
}

#[derive(Clone)]
pub struct JsRuntimeConfig {
    pub heap_size: usize,
    pub stack_size: usize,
    pub execution_timeout: Duration,
    /// Number of independent JS runtimes
    ///
    /// With a single worker, flows are processed one message at a time, in order.
    /// With more workers, flows with JS steps are processed concurrently,
    /// each JS runtime blocking a thread of the actor runtime while executing a script.
    pub workers: usize,
}

impl Default for JsRuntimeConfig {
    fn default() -> Self {
        JsRuntimeConfig {
            heap_size: 16 * 1024 * 1024,
            stack_size: 256 * 1024,
            execution_timeout: Duration::from_secs(5),
            workers: 1,
        }
    }
}

/// Time credits granted to a module evaluation and to a function call
const LOAD_CREDITS: usize = 100000;
const CALL_CREDITS: usize = 1000;

struct JsWorkerHandle {
    runtime: rquickjs::AsyncRuntime,
    requests: mpsc::Sender<JsRequest>,
}

impl JsWorkerHandle {
    async fn try_new(
        config: &JsRuntimeConfig,
        store: FlowContextHandle,
    ) -> Result<Self, LoadError> {
        let credits = Arc::new(AtomicUsize::new(LOAD_CREDITS));
        let runtime = Self::new_runtime(config, credits.clone()).await?;
        let context = rquickjs::AsyncContext::full(&runtime).await?;
        let requests = JsWorker::spawn(context, store, credits);
        Ok(JsWorkerHandle { runtime, requests })
    }

    async fn new_runtime(
        config: &JsRuntimeConfig,
        credits: Arc<AtomicUsize>,
    ) -> Result<rquickjs::AsyncRuntime, LoadError> {
        let runtime = rquickjs::AsyncRuntime::new()?;
        runtime.set_memory_limit(config.heap_size).await;
        runtime.set_max_stack_size(config.stack_size).await;
        runtime
            .set_interrupt_handler(Some(Box::new(move || {
                credits.fetch_sub(1, Ordering::Relaxed) == 0
            })))
            .await;
        Ok(runtime)
    }
}

impl JsRuntime {
    pub async fn with_default() -> Result<Self, LoadError> {
        Self::try_new(JsRuntimeConfig::default(), FlowContextHandle::default()).await
    }

    pub async fn with_config(config: JsRuntimeConfig) -> Result<Self, LoadError> {
        Self::try_new(config, FlowContextHandle::default()).await
    }

    pub async fn try_new(
        config: JsRuntimeConfig,
        store: FlowContextHandle,
    ) -> Result<Self, LoadError> {
        let mut workers = Vec::new();
        for _ in 0..config.workers.max(1) {
            workers.push(JsWorkerHandle::try_new(&config, store.clone()).await?);
        }
        Ok(JsRuntime {
            workers,
            store,
            flow_workers: HashMap::new(),
            module_sources: HashMap::new(),
            config,
        })
    }

    /// Return true if flows can be processed concurrently
    pub fn is_parallel(&self) -> bool {
        self.workers.len() > 1
    }

    /// Return the number of independent JS runtimes
    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    /// Release the runtimes assigned to a flow that has been removed
    pub fn release_flow(&mut self, flow: &Utf8Path) {
        self.flow_workers.retain(|(f, _), _| f != flow);
    }

    pub fn context_handle(&self) -> FlowContextHandle {
        self.store.clone()
    }

    pub async fn load_script(&mut self, script: &mut JsScript) -> Result<(), LoadError> {
        let path = script.path();
        let source = tokio::fs::read_to_string(path)
            .await
            .map_err(|err| LoadError::from_io(err, path))?;
        self.load_script_literal(script, source).await
    }

    pub async fn load_script_literal(
        &mut self,
        script: &mut JsScript,
        source: impl Into<Vec<u8>>,
    ) -> Result<(), LoadError> {
        let source = source.into();
        let analysis = SourceAnalysis::of(&String::from_utf8_lossy(&source));
        script.access = analysis.access;
        script.shares_globals = analysis.shares_globals;
        let exports = self
            .load_js(
                &script.flow,
                script.copy,
                analysis.shares_globals,
                script.module_name.to_owned(),
                source,
            )
            .await?;
        Self::set_exports(script, &exports);
        Ok(())
    }

    fn set_exports(script: &mut JsScript, exports: &[&str]) {
        for export in exports {
            match *export {
                "onMessage" => script.is_defined = true,
                "onInterval" => script.is_periodic = true,
                "onStartup" => script.has_startup = true,
                _ => (),
            }
        }
        if !script.is_defined {
            tracing::warn!(target: "flows", "Flow script with no 'onMessage' function: {}", script.path);
        }
    }

    async fn load_js(
        &mut self,
        flow: &Utf8Path,
        copy: usize,
        on_first_worker: bool,
        name: String,
        source: Vec<u8>,
    ) -> Result<Vec<&'static str>, LoadError> {
        if let Some(module) = self.module_sources.remove(&name) {
            // As rquickjs fails to drop old module versions,
            // the worker has to be re-created with fresh new Async Runtime & Context
            let worker = module.worker;
            self.workers[worker] =
                JsWorkerHandle::try_new(&self.config, self.store.clone()).await?;
            let modules: Vec<_> = self
                .module_sources
                .iter()
                .filter(|(_, m)| m.worker == worker)
                .map(|(n, m)| (n.to_owned(), m.source.clone()))
                .collect();
            for (n, s) in modules {
                self.load_new_js(worker, n, s).await?;
            }
        }

        let worker = self.flow_worker(flow, copy, on_first_worker);
        let exports = self
            .load_new_js(worker, name.clone(), source.clone())
            .await?;
        let flow = flow.to_string();
        self.module_sources.insert(
            name,
            ModuleSource {
                worker,
                flow,
                source,
            },
        );
        Ok(exports)
    }

    /// Return the worker assigned to a copy of a flow
    ///
    /// A new flow copy is assigned the least busy worker not already running another copy of the flow,
    /// unless it has to share `globalThis` with other flows, in which case it is assigned the first worker.
    fn flow_worker(&mut self, flow: &Utf8Path, copy: usize, on_first_worker: bool) -> usize {
        let key = (flow.to_owned(), copy);
        if let Some(worker) = self.flow_workers.get(&key) {
            if on_first_worker && *worker != 0 {
                tracing::warn!(target: "flows", "Flow {flow} now uses globalThis: it will only share globalThis with other flows once reloaded");
            }
            return *worker;
        }
        let worker = if on_first_worker {
            0
        } else {
            let mut flow_count = vec![0; self.workers.len()];
            let mut used_by_flow = vec![false; self.workers.len()];
            for ((f, _), worker) in self.flow_workers.iter() {
                flow_count[*worker] += 1;
                if f == flow {
                    used_by_flow[*worker] = true;
                }
            }
            (0..flow_count.len())
                .min_by_key(|w| (used_by_flow[*w], flow_count[*w]))
                .unwrap_or_default()
        };
        self.flow_workers.insert(key, worker);
        worker
    }

    async fn load_new_js(
        &self,
        worker: usize,
        name: String,
        source: impl Into<Vec<u8>>,
    ) -> Result<Vec<&'static str>, LoadError> {
        let (sender, receiver) = oneshot::channel();
        let source = source.into();
        let imports = vec!["onMessage", "onInterval", "onStartup"];
        self.send(
            worker,
            receiver,
            JsRequest::LoadModule {
                name,
                source,
                imports,
                sender,
            },
        )
        .await?
    }

    pub async fn call_function(
        &self,
        module: &str,
        function: &str,
        args: Vec<JsonValue>,
    ) -> Result<JsonValue, LoadError> {
        let Some(source) = self.module_sources.get(module) else {
            return Err(LoadError::UnknownModule {
                module_name: module.to_string(),
            });
        };
        let (sender, receiver) = oneshot::channel();
        self.send(
            source.worker,
            receiver,
            JsRequest::CallFunction {
                flow: source.flow.clone(),
                module: module.to_string(),
                function: function.to_string(),
                args,
                sender,
            },
        )
        .await?
    }

    pub async fn dump_memory_stats(&self) -> serde_json::Value {
        let mut malloc_bytes = 0;
        let mut memory_used_bytes = 0;
        let mut function_count = 0;
        let mut object_count = 0;
        for worker in &self.workers {
            let usage = worker.runtime.memory_usage().await;
            malloc_bytes += usage.malloc_size;
            memory_used_bytes += usage.memory_used_size;
            function_count += usage.js_func_count;
            object_count += usage.obj_count;
        }
        serde_json::json!({
            "malloc_bytes": malloc_bytes,
            "memory_used_bytes": memory_used_bytes,
            "function_count": function_count,
            "object_count": object_count,
        })
    }

    async fn send<Response>(
        &self,
        worker: usize,
        mut receiver: oneshot::Receiver<Response>,
        request: JsRequest,
    ) -> Result<Response, LoadError> {
        if self.workers[worker].requests.send(request).await.is_err() {
            panic!("No JS runtime");
        }

        // FIXME: The following timeout is not working
        //  - see unit test: js_script::while_loop
        //  - the issue is that the quickjs runtime fails to yield when executing `while(true)`
        //  - Using task::spawn_blocking to launch the quickjs runtime doesn't help
        //    - A timeout is the properly raised
        //    - but the JS runtime keeps executing `while(true)` and is no more responsive.
        match tokio::time::timeout(self.config.execution_timeout, &mut receiver).await {
            Ok(Err(_)) => panic!("JS runtime crashed"),
            Ok(Ok(response)) => Ok(response),
            Err(_) => Err(LoadError::Timeout),
        }
    }
}

enum JsRequest {
    LoadModule {
        name: String,
        source: Vec<u8>,
        imports: Vec<&'static str>,
        sender: oneshot::Sender<Result<Vec<&'static str>, LoadError>>,
    },
    CallFunction {
        flow: String,
        module: String,
        function: String,
        args: Vec<JsonValue>,
        sender: oneshot::Sender<Result<JsonValue, LoadError>>,
    },
}

struct JsWorker {
    context: rquickjs::AsyncContext,
    requests: mpsc::Receiver<JsRequest>,
    credits: Arc<AtomicUsize>,
}

impl JsWorker {
    pub fn spawn(
        context: rquickjs::AsyncContext,
        store: FlowContextHandle,
        credits: Arc<AtomicUsize>,
    ) -> mpsc::Sender<JsRequest> {
        let (sender, requests) = mpsc::channel(100);
        tokio::spawn(
            async move {
                let worker = JsWorker {
                    context,
                    requests,
                    credits,
                };
                worker.run(store).await
            }
            .instrument(tracing::Span::current()),
        );
        sender
    }

    async fn run(mut self, store: FlowContextHandle) {
        rquickjs::async_with!(self.context => |ctx| {
            js_lib::console::init(&ctx);
            js_lib::crypto::init(&ctx);
            js_lib::text_decoder::init(&ctx);
            js_lib::text_encoder::init(&ctx);
            store.init(&ctx);
            let calling_flow = CallingFlow::default();
            calling_flow.store_as_userdata(&ctx);
            let mut modules = JsModules::new();
            while let Some(request) = self.requests.recv().await {
                match request {
                    JsRequest::LoadModule{name, source, sender, imports} => {
                        self.credits.store(LOAD_CREDITS, Ordering::Relaxed);
                        let result = modules.load_module(ctx.clone(), name, source, imports).await;
                        let _ = sender.send(result);
                    }
                    JsRequest::CallFunction{flow, module, function, args, sender} => {
                        calling_flow.set(Some(flow));
                        self.credits.store(CALL_CREDITS, Ordering::Relaxed);
                        let result = modules.call_function(ctx.clone(), module, function, args).await;
                        calling_flow.set(None);
                        let _ = sender.send(result);
                    }
                }
            }
        })
        .await
    }
}

struct JsModules<'js> {
    modules: HashMap<String, Module<'js, Evaluated>>,
}

impl<'js> JsModules<'js> {
    fn new() -> Self {
        JsModules {
            modules: HashMap::new(),
        }
    }

    async fn load_module(
        &mut self,
        ctx: Ctx<'js>,
        name: String,
        source: Vec<u8>,
        imports: Vec<&'static str>,
    ) -> Result<Vec<&'static str>, LoadError> {
        debug!(target: "flows", "compile({name})");
        assert!(
            !self.modules.contains_key(&name),
            "reloading a module leaks memory"
        );
        let module = Module::declare(ctx.clone(), name.clone(), source)
            .map_err(|err| LoadError::from_js(&ctx, err))?;
        let (module, p) = module.eval().map_err(|err| LoadError::from_js(&ctx, err))?;
        let () = p.finish().map_err(|err| LoadError::from_js(&ctx, err))?;

        let mut exports = vec![];
        for import in imports {
            if let Ok(Some(v)) = module.get(import) {
                if rquickjs::Function::from_value(v).is_ok() {
                    exports.push(import);
                }
            }
        }

        self.modules.insert(name, module);
        Ok(exports)
    }

    async fn call_function(
        &mut self,
        ctx: Ctx<'js>,
        module_name: String,
        function: String,
        args: Vec<JsonValue>,
    ) -> Result<JsonValue, LoadError> {
        debug!(target: "flows", "link({module_name}.{function})");
        let module = self
            .modules
            .get(&module_name)
            .ok_or_else(|| LoadError::UnknownModule {
                module_name: module_name.clone(),
            })?;
        let f: rquickjs::Value = module
            .get(&function)
            .map_err(|_| LoadError::UnknownFunction {
                module_name: module_name.clone(),
                function: function.clone(),
            })?;
        let f = rquickjs::Function::from_value(f).map_err(|_| LoadError::UnknownFunction {
            module_name: module_name.clone(),
            function: function.clone(),
        })?;

        let r = match &args[..] {
            [] => f.call(()),
            [v0] => f.call((v0,)),
            [v0, v1] => f.call((v0, v1)),
            [v0, v1, v2] => f.call((v0, v1, v2)),
            [v0, v1, v2, v3] => f.call((v0, v1, v2, v3)),
            [v0, v1, v2, v3, v4] => f.call((v0, v1, v2, v3, v4)),
            [v0, v1, v2, v3, v4, v5] => f.call((v0, v1, v2, v3, v4, v5)),
            _ => unreachable!("tedge flows API doesn't have functions with >6 arguments"),
        };

        debug!(target: "flows", "execute({module_name}.{function}) => {r:?}");
        r.map_err(|err| LoadError::from_js(&ctx, err))
    }
}

impl LoadError {
    fn from_js(ctx: &Ctx<'_>, err: Error) -> Self {
        let caught = ctx.catch();
        if let Some(ex) = caught.as_exception() {
            LoadError::JsException {
                message: ex.message().unwrap_or_default(),
                stack: ex.stack().unwrap_or_default(),
            }
        } else if let Some(s) = caught.as_string() {
            // OOM: QuickJS couldn't allocate the Exception object and fell back to
            // storing "out of memory" as a string atom.
            LoadError::JsException {
                message: s.to_string().unwrap_or_default(),
                stack: String::new(),
            }
        } else if caught.is_null() && matches!(err, Error::Exception) {
            // Severe OOM: QuickJS couldn't allocate any exception value at all.
            LoadError::JsException {
                message: "out of memory".to_owned(),
                stack: String::new(),
            }
        } else {
            err.into()
        }
    }
}
