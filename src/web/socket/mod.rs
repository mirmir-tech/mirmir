mod message;

use std::time::Duration;

use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use tokio::time::MissedTickBehavior;
use tonic::Request;

use self::message::ServerMessage;
use super::{configuration::Configuration, security_headers, session, session::WebError, types};
use crate::{
    http::{ApiState, DashboardUpdate},
    rpc::{RuntimeService, proto, proto::runtime_server::Runtime},
};

pub async fn updates(
    State(state): State<ApiState>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response, WebError> {
    session::validate_origin(&headers)?;
    state.sessions().authenticate(&headers)?;
    let mut response = upgrade.on_upgrade(move |socket| serve(socket, state)).into_response();
    response.headers_mut().extend(security_headers());
    Ok(response)
}

async fn serve(mut socket: WebSocket, state: ApiState) {
    if send_initial(&mut socket, state.service()).await.is_err() {
        return;
    }
    let mut startup = state.service().watch_startup();
    let mut activity = state.service().watch_activity_updates();
    let mut shutdown = state.shutdown();
    let mut updates = state.updates();
    let mut telemetry = tokio::time::interval(Duration::from_secs(1));
    telemetry.set_missed_tick_behavior(MissedTickBehavior::Skip);
    telemetry.tick().await;
    loop {
        let event = tokio::select! {
            _ = telemetry.tick() => Next::Telemetry,
            result = startup.changed() => Next::Startup(result.is_ok()),
            event = activity.next() => Next::Activity(event),
            update = updates.recv() => Next::Dashboard(update),
            incoming = socket.recv() => Next::Incoming(incoming),
            _ = shutdown.changed() => Next::Shutdown,
        };
        if !handle(event, &mut socket, &state).await {
            break;
        }
    }
}

enum Next {
    Telemetry,
    Startup(bool),
    Activity(Option<Result<proto::ActivityEvent, tonic::Status>>),
    Dashboard(Result<DashboardUpdate, tokio::sync::broadcast::error::RecvError>),
    Incoming(Option<Result<Message, axum::Error>>),
    Shutdown,
}

async fn handle(event: Next, socket: &mut WebSocket, state: &ApiState) -> bool {
    let result = match event {
        Next::Telemetry => send_overview(socket, state.service()).await,
        Next::Startup(true) => send_startup(socket, state.service()).await,
        Next::Startup(false)
        | Next::Shutdown
        | Next::Activity(None)
        | Next::Dashboard(Err(tokio::sync::broadcast::error::RecvError::Closed))
        | Next::Incoming(Some(Ok(Message::Close(_)) | Err(_)) | None) => return false,
        Next::Activity(Some(Ok(event))) => send_activity(socket, state.service(), event).await,
        Next::Activity(Some(Err(error))) => {
            send(socket, ServerMessage::Error { message: error.message().to_owned() }).await
        },
        Next::Dashboard(
            Ok(DashboardUpdate::Configuration)
            | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)),
        ) => send_configuration(socket, state.service()).await,
        Next::Incoming(Some(Ok(Message::Ping(data)))) => socket.send(Message::Pong(data)).await,
        Next::Incoming(Some(Ok(_))) => Ok(()),
    };
    result.is_ok()
}

async fn send_initial(socket: &mut WebSocket, service: &RuntimeService) -> Result<(), axum::Error> {
    send_startup(socket, service).await?;
    send_overview(socket, service).await?;
    send_models(socket, service).await?;
    send_configuration(socket, service).await?;
    for event in service.activity_history() {
        send(socket, ServerMessage::Activity { activity: event.into() }).await?;
    }
    Ok(())
}

async fn send_startup(socket: &mut WebSocket, service: &RuntimeService) -> Result<(), axum::Error> {
    socket
        .send(Message::Text(
            serde_json::to_string(&ServerMessage::Startup {
                startup: service.startup_snapshot().into(),
            })
            .expect("websocket startup message should serialize")
            .into(),
        ))
        .await
}

async fn send_overview(
    socket: &mut WebSocket,
    service: &RuntimeService,
) -> Result<(), axum::Error> {
    let snapshot = service
        .telemetry(Request::new(proto::TelemetryRequest {}))
        .await
        .map_or_else(|error| Err(error.message().to_owned()), |value| Ok(value.into_inner()));
    match snapshot {
        Ok(snapshot) => send(socket, ServerMessage::Overview { overview: snapshot.into() }).await,
        Err(message) => send(socket, ServerMessage::Error { message }).await,
    }
}

async fn send_models(socket: &mut WebSocket, service: &RuntimeService) -> Result<(), axum::Error> {
    let response = service.list_local_models(Request::new(proto::ListLocalModelsRequest {})).await;
    match response {
        Ok(response) => {
            send(
                socket,
                ServerMessage::Models {
                    models: types::Models {
                        models: response.into_inner().models.into_iter().map(Into::into).collect(),
                    },
                },
            )
            .await
        },
        Err(error) => {
            send(socket, ServerMessage::Error { message: error.message().to_owned() }).await
        },
    }
}

async fn send_configuration(
    socket: &mut WebSocket,
    service: &RuntimeService,
) -> Result<(), axum::Error> {
    let response = service.get_configuration(Request::new(proto::GetConfigurationRequest {})).await;
    match response {
        Ok(response) => {
            send(
                socket,
                ServerMessage::Configuration {
                    configuration: Configuration::from(response.into_inner()),
                },
            )
            .await
        },
        Err(error) => {
            send(socket, ServerMessage::Error { message: error.message().to_owned() }).await
        },
    }
}

async fn send_activity(
    socket: &mut WebSocket,
    service: &RuntimeService,
    event: proto::ActivityEvent,
) -> Result<(), axum::Error> {
    let refresh_models = terminal(&event.state) && model_operation(&event.kind);
    send(socket, ServerMessage::Activity { activity: event.into() }).await?;
    if refresh_models {
        send_models(socket, service).await?;
    }
    Ok(())
}

async fn send(socket: &mut WebSocket, message: ServerMessage) -> Result<(), axum::Error> {
    let json = serde_json::to_string(&message).expect("websocket server message should serialize");
    socket.send(Message::Text(json.into())).await
}

fn terminal(state: &str) -> bool {
    matches!(state, "completed" | "failed" | "cancelled")
}

fn model_operation(kind: &str) -> bool {
    matches!(kind, "load" | "restore" | "unload" | "pull" | "remove")
}
