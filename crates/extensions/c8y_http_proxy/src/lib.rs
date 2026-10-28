use c8y_api::proxy_url::ProxyUrlGenerator;
use c8y_api::tenant_aliases::TenantAliases;

mod actor;
pub mod handle;
pub mod messages;

#[cfg(test)]
mod tests;

/// Configuration of C8Y REST API
#[derive(Clone)]
pub struct C8YHttpConfig {
    pub c8y_http_host: String,
    pub c8y_mqtt_host: String,
    pub device_id: String,
    proxy: ProxyUrlGenerator,
    tenant_aliases: TenantAliases,
}

impl C8YHttpConfig {
    pub fn new(
        device_id: String,
        c8y_http_host: String,
        c8y_mqtt_host: String,
        proxy: ProxyUrlGenerator,
    ) -> Self {
        C8YHttpConfig {
            c8y_http_host,
            c8y_mqtt_host,
            device_id,
            proxy,
            tenant_aliases: TenantAliases::default(),
        }
    }

    /// Routes through the local proxy the URLs using one of these tenant aliases
    pub fn with_tenant_aliases(self, tenant_aliases: TenantAliases) -> Self {
        C8YHttpConfig {
            tenant_aliases,
            ..self
        }
    }
}
