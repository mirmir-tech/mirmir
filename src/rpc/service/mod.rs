mod activity;
mod admission;
mod catalog;
mod configuration;
mod construction;
mod dashboard;
mod generation;
mod local_models;
mod models;
mod remote;
mod restore;
mod settings;
mod status;
mod tasks;
mod telemetry;

pub use telemetry::SAMPLING_INTERVAL_MS;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use super::proto;
use crate::{
    application::Application,
    remote::{Origin, RemoteAccess},
};

#[derive(Clone)]
pub struct RuntimeService {
    application: Application,
    remote: RemoteAccess,
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
        let health = Application::health();
        Ok(Response::new(proto::HealthResponse {
            protocol_version: health.protocol_version.to_owned(),
            server_version: health.server_version.to_owned(),
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
        let selectors = self.application.active_models().map_err(status::application)?;
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
        let selector = self.admitted(request, |message| &message.selector).await?.selector;
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
        let request = self.admitted(request, |message| &message.selector).await?;
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
        let request = self.admitted(request, |message| &message.selector).await?;
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
        match self.unload(&selector) {
            Ok(unloaded) => Ok(Response::new(proto::UnloadModelResponse { unloaded })),
            Err(error) => Err(error),
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
        Ok(Response::new(self.remove_request(request.into_inner().repo_id).await?))
    }

    async fn generate(
        &self,
        request: Request<proto::GenerateRequest>,
    ) -> Result<Response<Self::GenerateStream>, Status> {
        let request = self.admitted(request, |message| &message.model).await?;
        Ok(Response::new(self.generate_stream(request)))
    }

    async fn embed(
        &self,
        request: Request<proto::EmbedRequest>,
    ) -> Result<Response<proto::EmbedResponse>, Status> {
        let request = self.admitted(request, |message| &message.model).await?;
        self.embed_request(request).await.map(Response::new)
    }

    async fn rerank(
        &self,
        request: Request<proto::RerankRequest>,
    ) -> Result<Response<proto::RerankResponse>, Status> {
        let request = self.admitted(request, |message| &message.model).await?;
        self.rerank_request(request).await.map(Response::new)
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
        request: Request<proto::GetConfigurationRequest>,
    ) -> Result<Response<proto::ConfigurationSnapshot>, Status> {
        Ok(Response::new(configuration::snapshot(self, &Origin::of(&request))?))
    }

    async fn update_configuration(
        &self,
        request: Request<proto::UpdateConfigurationRequest>,
    ) -> Result<Response<proto::UpdateConfigurationResponse>, Status> {
        let origin = Origin::of(&request);
        Ok(Response::new(configuration::update(self, &origin, request.into_inner()).await?))
    }

    async fn watch_activity(
        &self,
        request: Request<proto::WatchActivityRequest>,
    ) -> Result<Response<Self::WatchActivityStream>, Status> {
        Ok(Response::new(activity::watch(
            &self.application,
            request.into_inner().include_history,
        )))
    }

    async fn cancel_operation(
        &self,
        request: Request<proto::CancelOperationRequest>,
    ) -> Result<Response<proto::CancelOperationResponse>, Status> {
        Ok(Response::new(activity::cancel(
            &self.application,
            &request.into_inner().operation_id,
        )))
    }

    async fn get_remote_access(
        &self,
        _request: Request<proto::GetRemoteAccessRequest>,
    ) -> Result<Response<proto::RemoteAccessStatus>, Status> {
        self.remote_status().await.map(Response::new)
    }

    async fn set_remote_access(
        &self,
        request: Request<proto::SetRemoteAccessRequest>,
    ) -> Result<Response<proto::RemoteAccessStatus>, Status> {
        self.set_remote(request.into_inner()).await.map(Response::new)
    }

    async fn create_remote_key(
        &self,
        request: Request<proto::CreateRemoteKeyRequest>,
    ) -> Result<Response<proto::CreateRemoteKeyResponse>, Status> {
        self.issue_remote_key(&request.into_inner()).map(Response::new)
    }

    async fn revoke_remote_key(
        &self,
        request: Request<proto::RevokeRemoteKeyRequest>,
    ) -> Result<Response<proto::RevokeRemoteKeyResponse>, Status> {
        self.withdraw_remote_key(&request.into_inner()).map(Response::new)
    }
}
