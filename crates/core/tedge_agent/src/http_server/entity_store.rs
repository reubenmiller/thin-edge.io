//! This module defines the axum routes and handlers for the entity store REST APIs.
//! The following endpoints are currently supported:
//!
//! - `POST /v1/entities`: Registers a new entity.
//! - `GET /v1/entities/*path`: Retrieves an existing entity.
//! - `DELETE /v1/entities/*path`: Deregisters an existing entity.
//!
//! References:
//!
//! - https://github.com/thin-edge/thin-edge.io/blob/main/design/decisions/0005-entity-registration-api.md
use super::server::AgentState;
use crate::entity_manager::server::EntityStoreRequest;
use crate::entity_manager::server::EntityStoreResponse;
use axum::extract::DefaultBodyLimit;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use axum::routing::post;
use axum::Json;
use axum::Router;
use hyper::StatusCode;
use serde::Deserialize;
use serde_json::json;
use serde_json::Map;
use serde_json::Value;
use std::str::FromStr;
use tedge_api::entity::EntityMetadata;
use tedge_api::entity::InvalidEntityType;
use tedge_api::entity_store;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::entity_store::EntityRegistrationPayload;
use tedge_api::entity_store::EntityTwinMessage;
use tedge_api::entity_store::EntityUpdateMessage;
use tedge_api::entity_store::ListFilters;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::TopicIdError;

pub const HTTP_MAX_PAYLOAD_SIZE: usize = 1048576; // 1 MB

#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    #[serde(default)]
    root: Option<String>,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum InputValidationError {
    #[error(transparent)]
    InvalidEntityType(#[from] InvalidEntityType),
    #[error(transparent)]
    InvalidEntityTopic(#[from] TopicIdError),
    #[error("The provided parameters: {0} and {1} are mutually exclusive. Use either one.")]
    IncompatibleParams(String, String),
}

impl TryFrom<ListParams> for ListFilters {
    type Error = InputValidationError;

    fn try_from(params: ListParams) -> Result<Self, Self::Error> {
        let root = params
            .root
            .filter(|v| !v.is_empty())
            .map(|val| val.parse())
            .transpose()?;
        let parent = params
            .parent
            .filter(|v| !v.is_empty())
            .map(|val| val.parse())
            .transpose()?;
        let r#type = params
            .r#type
            .filter(|v| !v.is_empty())
            .map(|val| val.parse())
            .transpose()?;

        if root.is_some() && parent.is_some() {
            return Err(InputValidationError::IncompatibleParams(
                "root".to_string(),
                "parent".to_string(),
            ));
        }

        Ok(Self {
            root,
            parent,
            r#type,
        })
    }
}

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error(transparent)]
    InvalidEntityTopicId(#[from] TopicIdError),

    #[allow(clippy::enum_variant_names)]
    #[error(transparent)]
    EntityStoreError(#[from] entity_store::Error),

    #[error("Entity with topic id: {0} not found")]
    EntityNotFound(EntityTopicId),

    #[allow(clippy::enum_variant_names)]
    #[error("Failed to publish entity registration message via MQTT")]
    ChannelError(#[from] tedge_actors::ChannelError),

    #[error("Received unexpected response from entity store")]
    InvalidEntityStoreResponse,

    #[error(transparent)]
    InvalidInput(#[from] InputValidationError),

    #[error(transparent)]
    FromSerdeJson(#[from] serde_json::Error),

    #[error("Not Found")]
    ResourceNotFound,

    #[error("Method Not Allowed")]
    MethodNotAllowed,

    #[error("Entity twin data for entity: {0} with fragment key: {1} not found")]
    EntityTwinDataNotFound(EntityTopicId, String),

    #[error("Actions on channel: {0} are not supported")]
    UnsupportedChannel(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status_code = match &self {
            Error::InvalidEntityTopicId(_) => StatusCode::BAD_REQUEST,
            Error::EntityStoreError(err) => match err {
                entity_store::Error::EntityAlreadyRegistered(_) => StatusCode::CONFLICT,
                entity_store::Error::UnknownEntity(_) => StatusCode::NOT_FOUND,
                entity_store::Error::NoParent(_) => StatusCode::BAD_REQUEST,
                entity_store::Error::UnknownHealthEndpoint(_) => StatusCode::BAD_REQUEST,
                entity_store::Error::InvalidTwinData(_) => StatusCode::BAD_REQUEST,
                _ => StatusCode::BAD_REQUEST,
            },
            Error::EntityNotFound(_) => StatusCode::NOT_FOUND,
            Error::ChannelError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::InvalidEntityStoreResponse => StatusCode::INTERNAL_SERVER_ERROR,
            Error::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Error::FromSerdeJson(_) => StatusCode::BAD_REQUEST,
            Error::ResourceNotFound => StatusCode::NOT_FOUND,
            Error::EntityTwinDataNotFound(_, _) => StatusCode::NOT_FOUND,
            Error::UnsupportedChannel(_) => StatusCode::NOT_FOUND,
            Error::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
        };
        let error_message = self.to_string();

        (status_code, Json(json!({ "error": error_message }))).into_response()
    }
}

pub(crate) fn entity_store_router(state: AgentState) -> Router {
    Router::new()
        .route("/v1/entities", post(register_entity).get(list_entities))
        .route(
            "/v1/entities/{*path}",
            get(get_resource)
                .put(put_resource)
                .patch(patch_resource)
                .delete(delete_resource),
        )
        .layer(DefaultBodyLimit::max(HTTP_MAX_PAYLOAD_SIZE))
        .with_state(state)
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct EntityRegistrationHttpPayload {
    #[serde(rename = "@topic-id")]
    pub topic_id: EntityTopicId,
    #[serde(flatten)]
    others: EntityRegistrationPayload,
}

impl From<EntityRegistrationHttpPayload> for EntityRegistrationMessage {
    fn from(value: EntityRegistrationHttpPayload) -> Self {
        EntityRegistrationMessage {
            topic_id: value.topic_id,
            external_id: value.others.external_id,
            r#type: value.others.r#type,
            parent: value.others.parent,
            health_endpoint: value.others.health_endpoint,
            twin_data: value.others.twin_data,
        }
    }
}

async fn register_entity(
    State(state): State<AgentState>,
    Json(payload): Json<EntityRegistrationHttpPayload>,
) -> impl IntoResponse {
    let reg_message: EntityRegistrationMessage = payload.into();
    let topic_id = reg_message.topic_id.clone();
    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::Create(reg_message))
        .await?;
    let EntityStoreResponse::Create(res) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };

    res?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"@topic-id": topic_id.as_str()})),
    ))
}

async fn get_resource(
    State(state): State<AgentState>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    let (topic_id, channel) = parse_path(&path)?;
    match channel {
        Channel::EntityMetadata => Ok(get_entity(state, topic_id).await.into_response()),
        Channel::EntityTwinData { fragment_key } => {
            if fragment_key.is_empty() {
                return Ok(get_entity_twin_fragments(state, topic_id)
                    .await
                    .into_response());
            }

            Ok(
                get_entity_twin_fragment(state, topic_id, fragment_key.to_string())
                    .await
                    .into_response(),
            )
        }
        _ => Err(Error::MethodNotAllowed),
    }
}

async fn put_resource(
    State(state): State<AgentState>,
    Path(path): Path<String>,
    payload: String,
) -> impl IntoResponse {
    let (topic_id, channel) = parse_path(&path)?;
    match channel {
        Channel::EntityTwinData { fragment_key } => {
            if fragment_key.is_empty() {
                let fragments = serde_json::from_str(&payload)?;
                return Ok(set_entity_twin_fragments(state, topic_id, fragments)
                    .await
                    .into_response());
            }

            let fragment_value: Value = serde_json::from_str(&payload)?;
            Ok(
                set_entity_twin_fragment(state, topic_id, fragment_key.to_string(), fragment_value)
                    .await
                    .into_response(),
            )
        }
        _ => Err(Error::MethodNotAllowed),
    }
}

async fn patch_resource(
    State(state): State<AgentState>,
    Path(path): Path<String>,
    payload: String,
) -> impl IntoResponse {
    let (topic_id, channel) = parse_path(&path)?;
    match channel {
        Channel::EntityMetadata => {
            let entity_update: EntityUpdateMessage = serde_json::from_str(&payload)?;
            Ok(update_entity(state, topic_id, entity_update)
                .await
                .into_response())
        }
        _ => Err(Error::MethodNotAllowed),
    }
}

async fn delete_resource(
    State(state): State<AgentState>,
    Path(path): Path<String>,
) -> Result<Response, Error> {
    let (topic_id, channel) = parse_path(&path)?;
    match channel {
        Channel::EntityMetadata => deregister_entity(state, topic_id).await,
        Channel::EntityTwinData { fragment_key } => {
            if fragment_key.is_empty() {
                return Ok(delete_entity_twin_fragments(state, topic_id)
                    .await
                    .into_response());
            }

            delete_entity_twin_fragment(state, topic_id, fragment_key.to_string()).await
        }
        _ => Err(Error::MethodNotAllowed),
    }
}

fn parse_path(path: &str) -> Result<(EntityTopicId, Channel), Error> {
    let segments = path.split('/').collect::<Vec<&str>>();
    match segments.as_slice() {
        [seg1, seg2] => {
            let topic_id = topic_id_from_path_segments(seg1, Some(seg2), None, None)?;
            Ok((topic_id, Channel::EntityMetadata))
        }
        [seg1, seg2, seg3] => {
            let topic_id = topic_id_from_path_segments(seg1, Some(seg2), Some(seg3), None)?;
            Ok((topic_id, Channel::EntityMetadata))
        }
        [seg1, seg2, seg3, seg4, "twin"] => {
            let topic_id = topic_id_from_path_segments(seg1, Some(seg2), Some(seg3), Some(seg4))?;
            Ok((
                topic_id,
                Channel::EntityTwinData {
                    fragment_key: "".to_string(),
                },
            ))
        }
        [seg1, seg2, seg3, seg4] => {
            let topic_id = topic_id_from_path_segments(seg1, Some(seg2), Some(seg3), Some(seg4))?;
            Ok((topic_id, Channel::EntityMetadata))
        }
        [seg1, seg2, seg3, seg4, "twin", fragment_key] => {
            let topic_id = topic_id_from_path_segments(seg1, Some(seg2), Some(seg3), Some(seg4))?;
            Ok((
                topic_id,
                Channel::EntityTwinData {
                    fragment_key: fragment_key.to_string(),
                },
            ))
        }
        [_, _, _, _, "twin", keys @ ..] => Err(Error::EntityStoreError(
            entity_store::Error::InvalidTwinData(keys.join("/")),
        )),
        [_, _, _, _, channel, ..] => Err(Error::UnsupportedChannel(channel.to_string())),
        _ => Err(Error::ResourceNotFound),
    }
}

fn topic_id_from_path_segments(
    seg1: &str,
    seg2: Option<&str>,
    seg3: Option<&str>,
    seg4: Option<&str>,
) -> Result<EntityTopicId, TopicIdError> {
    EntityTopicId::from_str(&format!(
        "{}/{}/{}/{}",
        seg1,
        seg2.unwrap_or_default(),
        seg3.unwrap_or_default(),
        seg4.unwrap_or_default()
    ))
}

async fn get_entity(
    state: AgentState,
    topic_id: EntityTopicId,
) -> Result<impl IntoResponse, Error> {
    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::Get(topic_id.clone()))
        .await?;

    let EntityStoreResponse::Get(entity_metadata) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };

    if let Some(entity) = entity_metadata {
        Ok(Json(entity))
    } else {
        Err(Error::EntityNotFound(topic_id))
    }
}

async fn update_entity(
    state: AgentState,
    topic_id: EntityTopicId,
    payload: EntityUpdateMessage,
) -> impl IntoResponse {
    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::Update(topic_id, payload))
        .await?;
    let EntityStoreResponse::Update(res) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };

    Ok(Json(res?))
}

async fn deregister_entity(state: AgentState, topic_id: EntityTopicId) -> Result<Response, Error> {
    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::Delete(topic_id.clone()))
        .await?;

    let EntityStoreResponse::Delete(deleted) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };

    if deleted.is_empty() {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }

    Ok((StatusCode::OK, Json(deleted)).into_response())
}

async fn list_entities(
    State(state): State<AgentState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<EntityMetadata>>, Error> {
    let filters = params.try_into()?;
    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::List(filters))
        .await?;

    let EntityStoreResponse::List(entities) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };

    Ok(Json(entities))
}

async fn get_entity_twin_fragment(
    state: AgentState,
    topic_id: EntityTopicId,
    fragment_key: String,
) -> Result<impl IntoResponse, Error> {
    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::GetTwinFragment(
            topic_id.clone(),
            fragment_key.clone(),
        ))
        .await?;
    let EntityStoreResponse::GetTwinFragment(fragment_value) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };

    if fragment_value.is_none() {
        return Err(Error::EntityTwinDataNotFound(topic_id, fragment_key));
    }

    Ok(Json(fragment_value))
}

async fn set_entity_twin_fragment(
    state: AgentState,
    topic_id: EntityTopicId,
    fragment_key: String,
    fragment_value: Value,
) -> impl IntoResponse {
    let twin_data = EntityTwinMessage::new(topic_id.clone(), fragment_key, fragment_value.clone());

    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::SetTwinFragment(twin_data))
        .await?;
    let EntityStoreResponse::SetTwinFragment(res) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };
    res?;

    Ok(Json(fragment_value))
}

async fn delete_entity_twin_fragment(
    state: AgentState,
    topic_id: EntityTopicId,
    fragment_key: String,
) -> Result<Response, Error> {
    let twin_data = EntityTwinMessage::new(topic_id.clone(), fragment_key, Value::Null);

    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::SetTwinFragment(twin_data))
        .await?;
    let EntityStoreResponse::SetTwinFragment(res) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };
    res?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn get_entity_twin_fragments(
    state: AgentState,
    topic_id: EntityTopicId,
) -> Result<impl IntoResponse, Error> {
    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::GetTwinFragments(topic_id.clone()))
        .await?;

    let EntityStoreResponse::GetTwinFragments(twin_data) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };

    Ok(Json(twin_data?))
}

async fn set_entity_twin_fragments(
    state: AgentState,
    topic_id: EntityTopicId,
    fragments: Map<String, Value>,
) -> impl IntoResponse {
    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::SetTwinFragments(
            topic_id,
            fragments.clone(),
        ))
        .await?;
    let EntityStoreResponse::SetTwinFragments(res) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };
    res?;

    Ok(Json(fragments))
}

async fn delete_entity_twin_fragments(
    state: AgentState,
    topic_id: EntityTopicId,
) -> impl IntoResponse {
    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::SetTwinFragments(topic_id, Map::new()))
        .await?;
    let EntityStoreResponse::SetTwinFragments(res) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };
    res?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::AgentState;
    use crate::entity_manager::server::EntityStoreRequest;
    use crate::entity_manager::server::EntityStoreResponse;
    use crate::http_server::entity_store::entity_store_router;
    use assert_json_diff::assert_json_eq;
    use axum::body::Body;
    use axum::response::Response;
    use axum::Router;
    use http_body_util::BodyExt as _;
    use hyper::Method;
    use hyper::Request;
    use hyper::StatusCode;
    use serde_json::json;
    use serde_json::Value;
    use std::collections::HashSet;
    use tedge_actors::Builder;
    use tedge_actors::ClientMessageBox;
    use tedge_actors::MessageReceiver;
    use tedge_actors::ServerMessageBox;
    use tedge_actors::ServerMessageBoxBuilder;
    use tedge_api::entity::EntityMetadata;
    use tedge_api::entity::EntityType;
    use tedge_api::entity_store;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_test_utils::fs::TempTedgeDir;
    use test_case::test_case;
    use tower::Service;

    #[tokio::test]
    async fn entity_get() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::Get(topic_id) = req.request {
                    if topic_id == EntityTopicId::default_child_device("test-child").unwrap() {
                        let entity =
                            EntityMetadata::child_device("test-child".to_string()).unwrap();
                        req.reply_to
                            .send(EntityStoreResponse::Get(Some(entity)))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let topic_id = "device/test-child//";
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/entities/{topic_id}"))
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: EntityMetadata = serde_json::from_slice(&body).unwrap();
        assert_eq!(entity.topic_id.as_str(), topic_id);
        assert_eq!(entity.r#type, EntityType::ChildDevice);
    }

    #[tokio::test]
    async fn get_unknown_entity() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::Get(topic_id) = req.request {
                    if topic_id == EntityTopicId::default_child_device("test-child").unwrap() {
                        req.reply_to
                            .send(EntityStoreResponse::Get(None))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let topic_id = "device/test-child//";
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/entities/{topic_id}"))
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: Value = serde_json::from_slice(&body).unwrap();
        assert_json_eq!(
            entity,
            json!({"error":"Entity with topic id: device/test-child// not found"})
        );
    }

    #[tokio::test]
    async fn entity_post() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::Create(entity) = req.request {
                    if entity.topic_id == EntityTopicId::default_child_device("test-child").unwrap()
                        && entity.r#type == EntityType::ChildDevice
                    {
                        req.reply_to
                            .send(EntityStoreResponse::Create(Ok(vec![])))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let payload = json!({
            "@topic-id": "device/test-child//",
            "@id": "test-child",
            "@type": "child-device",
        })
        .to_string();

        let req = Request::builder()
            .method(Method::POST)
            .uri("/v1/entities")
            .header("Content-Type", "application/json")
            .body(Body::from(payload))
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: Value = serde_json::from_slice(&body).unwrap();
        assert_json_eq!(entity, json!( {"@topic-id": "device/test-child//"}));
    }

    #[tokio::test]
    async fn entity_post_duplicate() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            let topic_id = EntityTopicId::default_child_device("test-child").unwrap();
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::Create(entity) = req.request {
                    if entity.topic_id == topic_id && entity.r#type == EntityType::ChildDevice {
                        req.reply_to
                            .send(EntityStoreResponse::Create(Err(
                                entity_store::Error::EntityAlreadyRegistered(topic_id),
                            )))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let payload = json!({
            "@topic-id": "device/test-child//",
            "@id": "test-child",
            "@type": "child-device",
        })
        .to_string();

        let req = Request::builder()
            .method(Method::POST)
            .uri("/v1/entities")
            .header("Content-Type", "application/json")
            .body(Body::from(payload))
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: Value = serde_json::from_slice(&body).unwrap();
        assert_json_eq!(
            entity,
            json!( {"error":"An entity with topic id: device/test-child// is already registered"})
        );
    }

    #[tokio::test]
    async fn entity_post_bad_parent() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            let topic_id = EntityTopicId::default_child_device("test-child").unwrap();
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::Create(entity) = req.request {
                    if entity.topic_id == topic_id && entity.r#type == EntityType::ChildDevice {
                        req.reply_to
                            .send(EntityStoreResponse::Create(Err(
                                entity_store::Error::NoParent(
                                    "test-child".to_string().into_boxed_str(),
                                ),
                            )))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let payload = json!({
            "@topic-id": "device/test-child//",
            "@id": "test-child",
            "@type": "child-device",
        })
        .to_string();

        let req = Request::builder()
            .method(Method::POST)
            .uri("/v1/entities")
            .header("Content-Type", "application/json")
            .body(Body::from(payload))
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: Value = serde_json::from_slice(&body).unwrap();
        assert_json_eq!(
            entity,
            json!( {"error":"The specified parent \"test-child\" does not exist in the entity store"})
        );
    }

    #[tokio::test]
    async fn entity_post_missing_topic_id() {
        let TestHandle {
            mut app,
            entity_store_box: _,
        } = setup();

        let payload = json!({
            "type": "child-device",
        })
        .to_string();

        let req = Request::builder()
            .method(Method::POST)
            .uri("/v1/entities")
            .header("Content-Type", "application/json")
            .body(Body::from(payload))
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = String::from_utf8(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        assert!(
            body.contains("missing field `@topic-id`"),
            "Expected error message to contain 'missing field `@topic-id`', but got: {body}"
        );
    }

    #[tokio::test]
    async fn entity_delete() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::Delete(topic_id) = req.request {
                    let target_entity =
                        EntityMetadata::child_device("test-child".to_string()).unwrap();
                    if topic_id == target_entity.topic_id {
                        req.reply_to
                            .send(EntityStoreResponse::Delete(vec![target_entity]))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let topic_id = "device/test-child//";
        let req = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/entities/{topic_id}"))
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let deleted: Vec<EntityMetadata> = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            deleted,
            vec![EntityMetadata::child_device("test-child".to_string()).unwrap()]
        );
    }

    #[tokio::test]
    async fn delete_unknown_entity_returns_no_content() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::Delete(topic_id) = req.request {
                    if topic_id == EntityTopicId::default_child_device("test-child").unwrap() {
                        req.reply_to
                            .send(EntityStoreResponse::Delete(vec![]))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let topic_id = "device/test-child//";
        let req = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/entities/{topic_id}"))
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn entity_list() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::List(_) = req.request {
                    req.reply_to
                        .send(EntityStoreResponse::List(vec![
                            EntityMetadata::main_device(None),
                            EntityMetadata::child_device("child0".to_string()).unwrap(),
                            EntityMetadata::child_device("child1".to_string()).unwrap(),
                        ]))
                        .await
                        .unwrap();
                }
            }
        });

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/entities")
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entities: Vec<EntityMetadata> = serde_json::from_slice(&body).unwrap();

        let entity_set = entities
            .iter()
            .map(|e| e.topic_id.as_str())
            .collect::<HashSet<_>>();
        assert!(entity_set.contains("device/main//"));
        assert!(entity_set.contains("device/child0//"));
        assert!(entity_set.contains("device/child1//"));
    }

    #[tokio::test]
    async fn entity_list_unknown_entity() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::List(_) = req.request {
                    req.reply_to
                        .send(EntityStoreResponse::List(vec![]))
                        .await
                        .unwrap();
                }
            }
        });

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/entities")
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entities: Vec<EntityMetadata> = serde_json::from_slice(&body).unwrap();
        assert!(entities.is_empty());
    }

    #[tokio::test]
    async fn entity_list_query_parameters() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::List(_) = req.request {
                    req.reply_to
                        .send(EntityStoreResponse::List(vec![
                            EntityMetadata::child_device("child00".to_string()).unwrap(),
                            EntityMetadata::child_device("child01".to_string()).unwrap(),
                        ]))
                        .await
                        .unwrap();
                }
            }
        });

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/entities?parent=device/child0//&type=child-device")
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entities: Vec<EntityMetadata> = serde_json::from_slice(&body).unwrap();

        let entity_set = entities
            .iter()
            .map(|e| e.topic_id.as_str())
            .collect::<HashSet<_>>();
        assert!(entity_set.contains("device/child00//"));
        assert!(entity_set.contains("device/child01//"));
    }

    #[tokio::test]
    async fn entity_list_empty_query_param() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();
        // Mock entity store actor response
        tokio::spawn(async move {
            while let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::List(_) = req.request {
                    req.reply_to
                        .send(EntityStoreResponse::List(vec![]))
                        .await
                        .unwrap();
                }
            }
        });

        for param in ["root=", "parent=", "type="].into_iter() {
            let uri = format!("/v1/entities?{}", param);
            let req = Request::builder()
                .method(Method::GET)
                .uri(uri)
                .body(Body::empty())
                .expect("request builder");

            let response = app.call(req).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/entities?root=&parent=&type=")
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn entity_list_bad_query_param() {
        let TestHandle {
            mut app,
            entity_store_box: _, // Not used
        } = setup();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/entities?parent=an/invalid/topic/id/")
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: Value = serde_json::from_slice(&body).unwrap();
        assert_json_eq!(
            entity,
            json!( {"error":"An entity topic identifier has at most 4 segments"})
        );
    }

    #[tokio::test]
    async fn entity_list_bad_query_parameter_combination() {
        let TestHandle {
            mut app,
            entity_store_box: _, // Not used
        } = setup();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/entities?root=device/some/topic/id&parent=device/another/topic/id")
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: Value = serde_json::from_slice(&body).unwrap();
        assert_json_eq!(
            entity,
            json!( {"error":"The provided parameters: root and parent are mutually exclusive. Use either one."})
        );
    }

    #[tokio::test]
    async fn entity_twin_get() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            while let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::GetTwinFragment(topic_id, fragment_key) = req.request {
                    if topic_id == EntityTopicId::default_child_device("test-child").unwrap()
                        && fragment_key == "foo"
                    {
                        req.reply_to
                            .send(EntityStoreResponse::GetTwinFragment(Some(json!("bar"))))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/entities/device/test-child///twin/foo")
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let fragment_value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(fragment_value, json!("bar"));
    }

    #[tokio::test]
    async fn entity_twin_get_unknown_entity() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::GetTwinFragment(topic_id, fragment_key) = req.request {
                    if topic_id == EntityTopicId::default_child_device("test-child").unwrap()
                        && fragment_key == "foo"
                    {
                        req.reply_to
                            .send(EntityStoreResponse::GetTwinFragment(None))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/entities/device/test-child///twin/foo")
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: Value = serde_json::from_slice(&body).unwrap();
        assert_json_eq!(
            entity,
            json!({"error":"Entity twin data for entity: device/test-child// with fragment key: foo not found"})
        );
    }

    #[tokio::test]
    async fn entity_twin_update() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            while let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::SetTwinFragment(twin_data) = req.request {
                    if twin_data.topic_id
                        == EntityTopicId::default_child_device("test-child").unwrap()
                        && twin_data.fragment_key == "foo"
                    {
                        req.reply_to
                            .send(EntityStoreResponse::SetTwinFragment(Ok(true)))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v1/entities/device/test-child///twin/foo")
            .header("Content-Type", "application/json")
            .body(Body::from(json!("bar").to_string()))
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let twin_value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(twin_value, json!("bar"));
    }

    #[test_case("@id"; "reserved")]
    #[test_case("with/slash"; "contains_slash")]
    #[tokio::test]
    async fn entity_twin_update_invalid_key(id: &str) {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::SetTwinFragment(twin_data) = req.request {
                    if twin_data.topic_id
                        == EntityTopicId::default_child_device("test-child").unwrap()
                        && twin_data.fragment_key == "@id"
                    {
                        req.reply_to
                            .send(EntityStoreResponse::SetTwinFragment(Err(
                                entity_store::Error::InvalidTwinData("@id".to_string()),
                            )))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/entities/device/test-child///twin/{id}"))
            .header("Content-Type", "application/json")
            .body(Body::from(json!("new-id").to_string()))
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: Value = serde_json::from_slice(&body).unwrap();
        let message = format!(
            "Invalid twin key: '{}'. Keys that are empty, containing '/' or starting with '@' are not allowed",
            id
        );
        assert_json_eq!(entity, json!({"error": message}));
    }

    #[tokio::test]
    async fn entity_twin_update_unknown_entity() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::SetTwinFragment(twin_data) = req.request {
                    if twin_data.topic_id
                        == EntityTopicId::default_child_device("test-child").unwrap()
                        && twin_data.fragment_key == "foo"
                    {
                        req.reply_to
                            .send(EntityStoreResponse::SetTwinFragment(Err(
                                entity_store::Error::UnknownEntity(
                                    "device/test-child//".to_string(),
                                ),
                            )))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v1/entities/device/test-child///twin/foo")
            .header("Content-Type", "application/json")
            .body(Body::from(json!("bar").to_string()))
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_non_existent_entity_response(response).await;
    }

    #[tokio::test]
    async fn entity_twin_delete() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            while let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::SetTwinFragment(twin_data) = req.request {
                    if twin_data.topic_id
                        == EntityTopicId::default_child_device("test-child").unwrap()
                        && twin_data.fragment_key == "foo"
                    {
                        req.reply_to
                            .send(EntityStoreResponse::SetTwinFragment(Ok(true)))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let req = Request::builder()
            .method(Method::DELETE)
            .uri("/v1/entities/device/test-child///twin/foo")
            .header("Content-Type", "application/json")
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn entity_twin_delete_non_existent_key() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            while let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::SetTwinFragment(twin_data) = req.request {
                    if twin_data.topic_id
                        == EntityTopicId::default_child_device("test-child").unwrap()
                        && twin_data.fragment_key == "foo"
                    {
                        req.reply_to
                            .send(EntityStoreResponse::SetTwinFragment(Ok(false)))
                            .await
                            .unwrap();
                    }
                } else if let EntityStoreRequest::Get(topic_id) = req.request {
                    if topic_id == EntityTopicId::default_child_device("test-child").unwrap() {
                        let entity =
                            EntityMetadata::child_device("test-child".to_string()).unwrap();

                        req.reply_to
                            .send(EntityStoreResponse::Get(Some(entity)))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let req = Request::builder()
            .method(Method::DELETE)
            .uri("/v1/entities/device/test-child///twin/foo")
            .header("Content-Type", "application/json")
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn entity_twin_delete_unknown_entity() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::SetTwinFragment(twin_data) = req.request {
                    if twin_data.topic_id
                        == EntityTopicId::default_child_device("test-child").unwrap()
                        && twin_data.fragment_key == "foo"
                    {
                        req.reply_to
                            .send(EntityStoreResponse::SetTwinFragment(Err(
                                entity_store::Error::UnknownEntity(
                                    "device/test-child//".to_string(),
                                ),
                            )))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let req = Request::builder()
            .method(Method::DELETE)
            .uri("/v1/entities/device/test-child///twin/foo")
            .header("Content-Type", "application/json")
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_non_existent_entity_response(response).await;
    }

    #[test_case(Method::GET; "GET")]
    #[test_case(Method::PUT; "PUT")]
    #[test_case(Method::DELETE; "DELETE")]
    #[tokio::test]
    async fn unsupported_channel(method: Method) {
        let TestHandle {
            mut app,
            entity_store_box: _, // Not used
        } = setup();

        let req = Request::builder()
            .method(method)
            .uri("/v1/entities/device/test-child///cmd/")
            .header("Content-Type", "application/json")
            .body(Body::from(json!("bar").to_string()))
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: Value = serde_json::from_slice(&body).unwrap();
        assert_json_eq!(
            entity,
            json!({"error":"Actions on channel: cmd are not supported"})
        );
    }

    #[tokio::test]
    async fn get_twin_fragments() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            while let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::GetTwinFragments(topic_id) = req.request {
                    if topic_id == EntityTopicId::default_child_device("test-child").unwrap() {
                        req.reply_to
                            .send(EntityStoreResponse::GetTwinFragments(Ok(
                                json!({"foo": "bar"}).as_object().unwrap().clone(),
                            )))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/entities/device/test-child///twin")
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let twin_data: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(twin_data, json!({"foo": "bar"}));
    }

    #[tokio::test]
    async fn get_twin_fragments_unknown_entity() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::GetTwinFragments(topic_id) = req.request {
                    if topic_id == EntityTopicId::default_child_device("test-child").unwrap() {
                        req.reply_to
                            .send(EntityStoreResponse::GetTwinFragments(Err(
                                entity_store::Error::UnknownEntity(
                                    "device/test-child//".to_string(),
                                ),
                            )))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/entities/device/test-child///twin")
            .body(Body::empty())
            .expect("request builder");
        let response = app.call(req).await.unwrap();
        assert_non_existent_entity_response(response).await;
    }

    #[tokio::test]
    async fn set_twin_fragments() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response for patch
        tokio::spawn(async move {
            while let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::SetTwinFragments(topic_id, _) = req.request {
                    if topic_id == EntityTopicId::default_child_device("test-child").unwrap() {
                        req.reply_to
                            .send(EntityStoreResponse::SetTwinFragments(Ok(())))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let twin_payload = json!({"foo": "bar"});

        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v1/entities/device/test-child///twin")
            .header("Content-Type", "application/json")
            .body(Body::from(twin_payload.to_string()))
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let twin_data: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(twin_data, twin_payload);
    }

    #[tokio::test]
    async fn set_twin_fragments_invalid_key() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::SetTwinFragments(topic_id, _) = req.request {
                    if topic_id == EntityTopicId::default_child_device("test-child").unwrap() {
                        req.reply_to
                            .send(EntityStoreResponse::SetTwinFragments(Err(
                                entity_store::Error::InvalidTwinData("@id".to_string()),
                            )))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v1/entities/device/test-child///twin")
            .header("Content-Type", "application/json")
            .body(Body::from(r#"{"@id": "new-id"}"#))
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: Value = serde_json::from_slice(&body).unwrap();
        assert_json_eq!(
            entity,
            json!({"error":"Invalid twin key: '@id'. Keys that are empty, containing '/' or starting with '@' are not allowed"})
        );
    }

    #[tokio::test]
    async fn set_twin_fragments_unknown_entity() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::SetTwinFragments(topic_id, _) = req.request {
                    if topic_id == EntityTopicId::default_child_device("test-child").unwrap() {
                        req.reply_to
                            .send(EntityStoreResponse::SetTwinFragments(Err(
                                entity_store::Error::UnknownEntity(
                                    "device/test-child//".to_string(),
                                ),
                            )))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v1/entities/device/test-child///twin")
            .header("Content-Type", "application/json")
            .body(Body::from(r#"{"foo": "bar"}"#))
            .expect("request builder");
        let response = app.call(req).await.unwrap();
        assert_non_existent_entity_response(response).await;
    }

    #[tokio::test]
    async fn set_twin_fragments_payload_too_large() {
        let TestHandle {
            mut app,
            entity_store_box: _,
        } = setup();

        let large_value = "x".repeat(1048576);
        let twin_payload = json!({"foo": large_value}).to_string();

        let req = Request::builder()
            .method(Method::PUT)
            .uri("/v1/entities/device/test-child///twin")
            .header("Content-Type", "application/json")
            .body(Body::from(twin_payload))
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    async fn assert_non_existent_entity_response(response: Response<Body>) {
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: Value = serde_json::from_slice(&body).unwrap();
        assert_json_eq!(
            entity,
            json!({"error":"The specified entity: device/test-child// does not exist in the entity store"})
        );
    }

    struct TestHandle {
        app: Router,
        entity_store_box: ServerMessageBox<EntityStoreRequest, EntityStoreResponse>,
    }

    fn setup() -> TestHandle {
        let ttd: TempTedgeDir = TempTedgeDir::new();
        let file_transfer_dir = ttd.utf8_path_buf();

        let mut entity_store_box = ServerMessageBoxBuilder::new("EntityStoreBox", 16);
        let entity_store_handle = ClientMessageBox::new(&mut entity_store_box);

        let agent_state = AgentState {
            file_transfer_dir,
            entity_store_handle,
        };
        // TODO: Add a timeout to this router. Attempts to add a tower_http::timer::TimeoutLayer as a layer failed.
        let app: Router = entity_store_router(agent_state);

        TestHandle {
            app,
            entity_store_box: entity_store_box.build(),
        }
    }
}
