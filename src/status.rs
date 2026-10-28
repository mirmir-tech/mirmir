use serde::Serialize;

use crate::{
    config::Paths,
    error::Result,
    instance::Instance,
    output,
    rpc::{self, proto},
};

#[derive(Serialize)]
struct Status {
    running: bool,
    socket: String,
    protocol_version: Option<String>,
    server_version: Option<String>,
    active_models: Vec<String>,
    telemetry: Option<proto::TelemetrySnapshot>,
}

pub async fn run(instance: &Instance) -> Result<()> {
    let (mut client, socket) = if let Some(socket) = instance.attached_socket() {
        (instance.connect_existing().await?, socket.to_owned())
    } else {
        let paths = Paths::discover()?;
        let client = match rpc::connect(&paths.socket_file).await {
            Ok(client) => client,
            Err(error) if rpc::unavailable(&error) => {
                return output::json(&Status::stopped(&paths));
            },
            Err(error) => return Err(error),
        };
        (client, paths.socket_file)
    };
    let health = client.health(proto::HealthRequest {}).await?.into_inner();
    let active = client.list_active_models(proto::ListActiveModelsRequest {}).await?.into_inner();
    let telemetry = client.telemetry(proto::TelemetryRequest {}).await?.into_inner();
    output::json(&Status {
        running: true,
        socket: socket.display().to_string(),
        protocol_version: Some(health.protocol_version),
        server_version: Some(health.server_version),
        active_models: active.selectors,
        telemetry: Some(telemetry),
    })
}

impl Status {
    fn stopped(paths: &Paths) -> Self {
        Self {
            running: false,
            socket: paths.socket_file.display().to_string(),
            protocol_version: None,
            server_version: None,
            active_models: Vec::new(),
            telemetry: None,
        }
    }
}
