use crate::access::ContextAccess;
use crate::js_value::JsonValue;
use rquickjs::class::Trace;
use rquickjs::Ctx;
use rquickjs::IntoJs;
use rquickjs::JsLifetime;
use rquickjs::Object;
use rquickjs::Result;
use rquickjs::Value;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::warn;

#[derive(Clone, Debug, Default, JsLifetime)]
pub struct FlowContextHandle {
    handle: Arc<Mutex<LayeredKVStore>>,
    access: Arc<AccessMonitor>,
}

/// Tracks how each flow is allowed to access the mapper context
///
/// The class declared for a flow when loaded is raised
/// when the flow is caught accessing the mapper context in a way its source doesn't show.
#[derive(Debug, Default)]
struct AccessMonitor {
    /// The class of each flow, as declared or raised
    flows: Mutex<HashMap<String, ContextAccess>>,
    /// Incremented each time the class of a flow is raised
    generation: AtomicUsize,
}

/// The flow whose function is being executed by a JS runtime
#[derive(Clone, Debug, Default, JsLifetime)]
pub(crate) struct CallingFlow {
    flow: Arc<Mutex<Option<String>>>,
}

impl CallingFlow {
    pub(crate) fn set(&self, flow: Option<String>) {
        *self.flow.lock().unwrap() = flow;
    }

    pub(crate) fn store_as_userdata(&self, ctx: &Ctx<'_>) {
        let _ = ctx.store_userdata(self.clone());
    }

    fn get_from_userdata(ctx: &Ctx<'_>) -> Option<String> {
        ctx.userdata::<Self>()
            .and_then(|calling| calling.flow.lock().unwrap().clone())
    }
}

#[derive(Default, Debug)]
struct LayeredKVStore {
    global: BTreeMap<String, JsonValue>,
    scoped: HashMap<FlowContext, BTreeMap<String, JsonValue>>,
    updates: Vec<FlowContextUpdate>,
}

#[derive(Debug)]
pub enum FlowContextUpdate {
    Inserted { key: String },
    Removed { key: String },
}

pub trait KVStore: Send + Sync {
    fn get_value(&self, key: &str) -> JsonValue;
    fn set_value(&mut self, key: &str, value: JsonValue);
    fn get_keys(&self) -> Vec<String>;
}

impl FlowContextHandle {
    pub fn get_value(&self, key: &str) -> JsonValue {
        self.get(&FlowContext::Mapper, key)
    }

    pub fn set_value(&self, key: &str, value: JsonValue) {
        self.insert(&FlowContext::Mapper, key, value);
    }

    pub fn get_keys(&self) -> Vec<String> {
        self.keys(&FlowContext::Mapper)
    }

    pub(crate) fn init(&self, ctx: &Ctx<'_>) {
        self.store_as_userdata(ctx)
    }

    /// Declare how a flow accesses the mapper context, resetting any class raised previously
    pub(crate) fn declare_access(&self, flow: &str, access: ContextAccess) {
        self.access
            .flows
            .lock()
            .unwrap()
            .insert(flow.to_owned(), access);
    }

    /// Return the class of a flow, as declared or raised after a misclassified access
    pub(crate) fn flow_access(&self, flow: &str, declared: ContextAccess) -> ContextAccess {
        match self.access.flows.lock().unwrap().get(flow) {
            Some(access) => declared.max(*access),
            None => declared,
        }
    }

    /// A number that changes each time the class of a flow is raised
    pub(crate) fn access_generation(&self) -> usize {
        self.access.generation.load(Ordering::Acquire)
    }

    /// Check an access to the mapper context by a flow, raising the class of the flow if needed
    fn check_access(&self, flow: &str, access: ContextAccess) {
        let mut flows = self.access.flows.lock().unwrap();
        let Some(flow_access) = flows.get_mut(flow) else {
            return;
        };
        if access > *flow_access {
            warn!(target: "flows", "Flow {flow} accesses context.mapper in a way its source doesn't show: it is now ordered as a {access} of context.mapper");
            *flow_access = access;
            self.access.generation.fetch_add(1, Ordering::Release);
        }
    }

    fn check_js_access(&self, ctx: &Ctx<'_>, context: &FlowContext, access: ContextAccess) {
        if context.is_global() {
            if let Some(flow) = CallingFlow::get_from_userdata(ctx) {
                self.check_access(&flow, access)
            }
        }
    }

    pub(crate) fn js_context<'js>(
        ctx: &Ctx<'js>,
        flow_name: &str,
        script_name: &str,
        config: &JsonValue,
    ) -> Result<Value<'js>> {
        let context = Object::new(ctx.clone())?;

        context.set("mapper", FlowContext::Mapper)?;
        context.set("flow", FlowContext::flow(flow_name))?;
        context.set("script", FlowContext::script(script_name))?;
        context.set("config", config)?;

        context.into_js(ctx)
    }

    pub(crate) fn get(&self, context: &FlowContext, key: &str) -> JsonValue {
        self.handle.lock().unwrap().get(context, key)
    }

    pub(crate) fn insert(&self, context: &FlowContext, key: &str, value: impl Into<JsonValue>) {
        let mut data = self.handle.lock().unwrap();
        data.update(context, key, value);
    }

    pub(crate) fn keys(&self, context: &FlowContext) -> Vec<String> {
        self.handle.lock().unwrap().keys(context)
    }

    pub(crate) fn remove(&self, context: &FlowContext, key: &str) {
        let mut data = self.handle.lock().unwrap();
        data.remove(context, key);
    }

    pub(crate) fn drain_updates(&mut self) -> Vec<FlowContextUpdate> {
        let mut data = self.handle.lock().unwrap();
        std::mem::take(&mut data.updates)
    }

    pub(crate) fn store_as_userdata(&self, ctx: &Ctx<'_>) {
        let _ = ctx.store_userdata(self.clone());
    }

    pub(crate) fn get_from_userdata(ctx: &Ctx<'_>) -> Self {
        match ctx.userdata::<Self>() {
            None => panic!("tedge-flows context has not been initialized!"),
            Some(userdata) => userdata.deref().clone(),
        }
    }
}

impl LayeredKVStore {
    fn context(&self, context: &FlowContext) -> Option<&impl KVStore> {
        if context.is_global() {
            Some(&self.global)
        } else {
            self.scoped.get(context)
        }
    }

    fn context_mut(&mut self, context: &FlowContext) -> Option<&mut impl KVStore> {
        if context.is_global() {
            Some(&mut self.global)
        } else {
            self.scoped.get_mut(context)
        }
    }

    fn entry(&mut self, context: &FlowContext) -> &mut impl KVStore {
        if context.is_global() {
            &mut self.global
        } else {
            self.scoped.entry(context.clone()).or_default()
        }
    }

    fn get(&self, context: &FlowContext, key: &str) -> JsonValue {
        match self.context(context) {
            None => JsonValue::Null,
            Some(map) => map.get_value(key),
        }
    }

    fn update(&mut self, context: &FlowContext, key: &str, value: impl Into<JsonValue>) {
        match value.into() {
            JsonValue::Null => self.remove(context, key),
            value => self.insert(context, key, value),
        }
    }

    fn keys(&self, context: &FlowContext) -> Vec<String> {
        match self.context(context) {
            None => vec![],
            Some(map) => map.get_keys(),
        }
    }

    fn insert(&mut self, context: &FlowContext, key: &str, value: JsonValue) {
        self.entry(context).set_value(key, value);
        if context.is_global() {
            self.updates.push(FlowContextUpdate::Inserted {
                key: key.to_string(),
            });
        }
    }

    pub fn remove(&mut self, context: &FlowContext, key: &str) {
        if let Some(map) = self.context_mut(context) {
            map.set_value(key, JsonValue::Null);
            if context.is_global() {
                self.updates.push(FlowContextUpdate::Removed {
                    key: key.to_string(),
                });
            }
        }
    }
}

#[derive(Clone, Debug, Trace, JsLifetime, Hash, Eq, PartialEq)]
#[rquickjs::class(frozen)]
pub(crate) enum FlowContext {
    Mapper,
    Flow(String),
    Script(String),
}

impl FlowContext {
    pub(crate) fn flow(name: &str) -> Self {
        FlowContext::Flow(name.to_owned())
    }

    pub(crate) fn script(name: &str) -> Self {
        FlowContext::Script(name.to_owned())
    }

    fn is_global(&self) -> bool {
        self == &FlowContext::Mapper
    }
}

#[rquickjs::methods]
impl<'js> FlowContext {
    fn get(&self, ctx: Ctx<'js>, key: String) -> Result<JsonValue> {
        let data = FlowContextHandle::get_from_userdata(&ctx);
        data.check_js_access(&ctx, self, ContextAccess::Read);
        Ok(data.get(self, &key))
    }

    fn set(&self, ctx: Ctx<'js>, key: String, value: JsonValue) {
        let data = FlowContextHandle::get_from_userdata(&ctx);
        data.check_js_access(&ctx, self, ContextAccess::Write);
        data.insert(self, &key, value)
    }

    fn remove(&self, ctx: Ctx<'js>, key: String) {
        let data = FlowContextHandle::get_from_userdata(&ctx);
        data.check_js_access(&ctx, self, ContextAccess::Write);
        data.remove(self, &key)
    }

    fn keys(&self, ctx: Ctx<'js>) -> Vec<String> {
        let data = FlowContextHandle::get_from_userdata(&ctx);
        data.check_js_access(&ctx, self, ContextAccess::Read);
        data.keys(self)
    }
}

impl KVStore for BTreeMap<String, JsonValue> {
    fn get_value(&self, key: &str) -> JsonValue {
        self.get(key).cloned().unwrap_or(JsonValue::Null)
    }

    fn set_value(&mut self, key: &str, value: JsonValue) {
        match value {
            JsonValue::Null => {
                self.remove(key);
            }
            value => {
                self.insert(key.to_owned(), value);
            }
        }
    }

    fn get_keys(&self) -> Vec<String> {
        self.keys().cloned().collect()
    }
}

impl KVStore for FlowContextHandle {
    fn get_value(&self, key: &str) -> JsonValue {
        self.get(&FlowContext::Mapper, key)
    }

    fn set_value(&mut self, key: &str, value: JsonValue) {
        self.insert(&FlowContext::Mapper, key, value);
    }

    fn get_keys(&self) -> Vec<String> {
        self.keys(&FlowContext::Mapper)
    }
}
