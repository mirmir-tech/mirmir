mod activity;
mod catalog;
mod configuration;
mod construction;
mod dashboard;
mod generation;
mod local_models;
mod models;
mod preflight;
mod presentation;
mod restore;
mod settings;
mod startup;
mod status;
mod tasks;
mod telemetry;

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use libmir::Library;
pub use startup::Snapshot as StartupSnapshot;
pub use telemetry::history::SAMPLING_INTERVAL_MS;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use self::{activity::Activity, models::ModelEntry, startup::Startup, telemetry::Telemetry};
use super::{PROTOCOL_VERSION, proto};
use crate::{catalog::Catalog, config::Store};

#[derive(Clone)]
pub struct RuntimeService {
    library: Library,
    store: Store,
    catalog: Catalog,
    models: Arc<Mutex<HashMap<String, ModelEntry>>>,
    loading: Arc<Mutex<HashSet<String>>>,
    model_memory_gate: Arc<Mutex<()>>,
    model_residency: models::ModelResidency,
    telemetry: Telemetry,
    activity: Activity,
    startup: Startup,
}

#[tonic::async_trait]
impl proto::runtime_server::Runtime for RuntimeService {
    type GenerateStream = ReceiverStream<Result<proto::GenerateEvent, Status>>;
    type LoadModelStream = ReceiverStream<Result<proto::ModelLifecycleEvent, Status>>;
    type PullModelStream = ReceiverStream<Result<proto::ModelTransferEvent, Status>>;
    type WatchActivityStream = ReceiverStream<Result<proto::ActivityEvent, Status>>;

    async fn health(
        &self,
        _request: Request<proto::HealthRequest>,
    ) -> Result<Response<proto::HealthResponse>, Status> {
        Ok(Response::new(proto::HealthResponse {
            protocol_version: PROTOCOL_VERSION.to_owned(),
            server_version: env!("CARGO_PKG_VERSION").to_owned(),
        }))
    }

    async fn list_models(
        &self,
        _request: Request<proto::ListModelsRequest>,
    ) -> Result<Response<proto::ListModelsResponse>, Status> {
        Ok(Response::new(proto::ListModelsResponse { models: self.list()? }))
    }

    async fn list_active_models(
        &self,
        _request: Request<proto::ListActiveModelsRequest>,
    ) -> Result<Response<proto::ListActiveModelsResponse>, Status> {
        let selectors = status::internal(self.store.active_models())?;
        self.report_state_recovery()?;
        Ok(Response::new(proto::ListActiveModelsResponse { selectors }))
    }

    async fn list_local_models(
        &self,
        _request: Request<proto::ListLocalModelsRequest>,
    ) -> Result<Response<proto::ListLocalModelsResponse>, Status> {
        let models = self.local_models()?;
        self.report_state_recovery()?;
        Ok(Response::new(proto::ListLocalModelsResponse { models }))
    }

    async fn inspect_model(
        &self,
        request: Request<proto::InspectModelRequest>,
    ) -> Result<Response<proto::InspectModelResponse>, Status> {
        let selector = request.into_inner().selector;
        let service = self.clone();
        let inspected = status::internal(
            tokio::task::spawn_blocking(move || service.inspect_model(&selector)).await,
        )??;
        Ok(Response::new(inspected))
    }

    async fn update_model_generation(
        &self,
        request: Request<proto::UpdateModelGenerationRequest>,
    ) -> Result<Response<proto::UpdateModelGenerationResponse>, Status> {
        let request = request.into_inner();
        let settings = request
            .settings
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("generation settings are required"))?;
        let model_id = self.save_generation_defaults(&request.selector, settings)?;
        tracing::info!(model = %model_id, "model generation defaults updated");
        Ok(Response::new(proto::UpdateModelGenerationResponse { model_id }))
    }

    async fn load_model(
        &self,
        request: Request<proto::LoadModelRequest>,
    ) -> Result<Response<Self::LoadModelStream>, Status> {
        let request = request.into_inner();
        let service = self.clone();
        let (sender, receiver) = mpsc::channel(64);
        drop(tokio::task::spawn_blocking(move || {
            models::stream_load(&service, &request, &sender);
        }));
        Ok(Response::new(ReceiverStream::new(receiver)))
    }

    async fn unload_model(
        &self,
        request: Request<proto::UnloadModelRequest>,
    ) -> Result<Response<proto::UnloadModelResponse>, Status> {
        let selector = request.into_inner().selector;
        let operation = self.activity.begin("unload", &selector, None);
        match self.unload(&selector) {
            Ok(unloaded) => {
                operation.finish(
                    "completed",
                    if unloaded {
                        "model unloaded"
                    } else {
                        "not loaded"
                    },
                );
                Ok(Response::new(proto::UnloadModelResponse { unloaded }))
            },
            Err(error) => {
                operation.finish("failed", error.message());
                Err(error)
            },
        }
    }

    async fn search_models(
        &self,
        request: Request<proto::SearchModelsRequest>,
    ) -> Result<Response<proto::SearchModelsResponse>, Status> {
        Ok(Response::new(catalog::search(self, request.into_inner()).await?))
    }

    async fn pull_model(
        &self,
        request: Request<proto::PullModelRequest>,
    ) -> Result<Response<Self::PullModelStream>, Status> {
        Ok(Response::new(catalog::pull_stream(self.clone(), request.into_inner())?))
    }

    async fn remove_model(
        &self,
        request: Request<proto::RemoveModelRequest>,
    ) -> Result<Response<proto::RemoveModelResponse>, Status> {
        Ok(Response::new(self.remove_with_activity(request.into_inner().repo_id).await?))
    }

    async fn generate(
        &self,
        request: Request<proto::GenerateRequest>,
    ) -> Result<Response<Self::GenerateStream>, Status> {
        Ok(Response::new(generation::stream(self.clone(), request.into_inner())))
    }

    async fn embed(
        &self,
        request: Request<proto::EmbedRequest>,
    ) -> Result<Response<proto::EmbedResponse>, Status> {
        tasks::embed_rpc(self.clone(), request.into_inner()).await.map(Response::new)
    }

    async fn rerank(
        &self,
        request: Request<proto::RerankRequest>,
    ) -> Result<Response<proto::RerankResponse>, Status> {
        tasks::rerank_rpc(self.clone(), request.into_inner()).await.map(Response::new)
    }

    async fn telemetry(
        &self,
        _request: Request<proto::TelemetryRequest>,
    ) -> Result<Response<proto::TelemetrySnapshot>, Status> {
        Ok(Response::new(self.telemetry_snapshot()?))
    }

    async fn telemetry_history(
        &self,
        request: Request<proto::TelemetryHistoryRequest>,
    ) -> Result<Response<proto::TelemetryHistoryResponse>, Status> {
        Ok(Response::new(self.telemetry_history_response(request.into_inner().limit)?))
    }

    async fn get_configuration(
        &self,
        _request: Request<proto::GetConfigurationRequest>,
    ) -> Result<Response<proto::ConfigurationSnapshot>, Status> {
        Ok(Response::new(configuration::snapshot(self)?))
    }

    async fn update_configuration(
        &self,
        request: Request<proto::UpdateConfigurationRequest>,
    ) -> Result<Response<proto::UpdateConfigurationResponse>, Status> {
        Ok(Response::new(configuration::update(self, request.into_inner()).await?))
    }

    async fn watch_activity(
        &self,
        request: Request<proto::WatchActivityRequest>,
    ) -> Result<Response<Self::WatchActivityStream>, Status> {
        Ok(Response::new(self.activity.watch(request.into_inner().include_history)))
    }

    async fn cancel_operation(
        &self,
        request: Request<proto::CancelOperationRequest>,
    ) -> Result<Response<proto::CancelOperationResponse>, Status> {
        let operation_id = request.into_inner().operation_id;
        let response = self.activity.cancel(&operation_id);
        tracing::info!(
            operation = %operation_id,
            found = response.found,
            accepted = response.accepted,
            state = %response.state,
            "operation cancellation requested"
        );
        Ok(Response::new(response))
    }
}
