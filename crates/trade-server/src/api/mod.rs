use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use warp::Filter;

pub mod core_handler;
pub mod types;

#[async_trait]
pub trait ApiHandler: Send + Sync {
    fn method_prefix(&self) -> &str;

    async fn handle_request(&self, method: &str, params: Option<Value>) -> Result<Value>;

    fn list_methods(&self) -> Vec<String>;
}

pub struct ApiServer {
    handlers: Arc<RwLock<HashMap<String, Arc<dyn ApiHandler>>>>,
}

impl ApiServer {
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_handler(&self, handler: Arc<dyn ApiHandler>) -> Result<()> {
        let prefix = handler.method_prefix();
        let mut handlers = self.handlers.write().await;
        handlers.insert(prefix.to_string(), handler);
        Ok(())
    }

    pub async fn handle_json_rpc_request(
        &self,
        request: types::JsonRpcRequest,
    ) -> types::JsonRpcResponse {
        let method_parts: Vec<&str> = request.method.split('_').collect();
        if method_parts.len() < 2 {
            return types::JsonRpcResponse::error(
                request.id,
                types::JsonRpcError {
                    code: types::METHOD_NOT_FOUND,
                    message: "Method not found".to_string(),
                    data: None,
                },
            );
        }

        let prefix = method_parts[0];
        let handlers = self.handlers.read().await;

        match handlers.get(prefix) {
            Some(handler) => {
                match handler
                    .handle_request(&request.method, request.params)
                    .await
                {
                    Ok(result) => types::JsonRpcResponse::success(request.id, result),
                    Err(e) => types::JsonRpcResponse::error(
                        request.id,
                        types::JsonRpcError {
                            code: types::INTERNAL_ERROR,
                            message: format!("Internal error: {}", e),
                            data: None,
                        },
                    ),
                }
            }
            None => types::JsonRpcResponse::error(
                request.id,
                types::JsonRpcError {
                    code: types::METHOD_NOT_FOUND,
                    message: "Method not found".to_string(),
                    data: None,
                },
            ),
        }
    }

    pub fn build_routes(
        self: Arc<Self>,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        let api_server = self.clone();
        let json_rpc = warp::path("api")
            .and(warp::post())
            .and(warp::body::json())
            .and_then(move |request: types::JsonRpcRequest| {
                let server = api_server.clone();
                async move {
                    let response = server.handle_json_rpc_request(request).await;
                    Ok::<_, warp::Rejection>(warp::reply::json(&response))
                }
            });

        let api_server = self.clone();
        let methods_list = warp::path!("api" / "methods")
            .and(warp::get())
            .and_then(move || {
                let server = api_server.clone();
                async move {
                    let handlers = server.handlers.read().await;
                    let mut all_methods = Vec::new();

                    for handler in handlers.values() {
                        all_methods.extend(handler.list_methods());
                    }

                    all_methods.sort();
                    Ok::<_, warp::Rejection>(warp::reply::json(&all_methods))
                }
            });

        json_rpc.or(methods_list)
    }
}

impl Default for ApiServer {
    fn default() -> Self {
        Self::new()
    }
}
