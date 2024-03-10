use crate::HeatingState;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::Router;
use log::info;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;

pub const HEATING_IS_ON_ROUTE: &str = "/heating_is_on";
pub const CURRENT_TEMP_ROUTE: &str = "/temp/:temp";

pub async fn start_server(port: String, heating_state: Arc<Mutex<HeatingState>>) {
    let app = Router::new()
        .route(HEATING_IS_ON_ROUTE, get(heating_is_on))
        .route(CURRENT_TEMP_ROUTE, post(receive_temp))
        .with_state(heating_state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();

    info!("Listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
async fn heating_is_on(State(heating_state): State<Arc<Mutex<HeatingState>>>) -> String {
    let heating_state = heating_state.lock().await;

    if SystemTime::now()
        .duration_since(heating_state.current_temp_reported_at)
        .unwrap()
        > Duration::from_secs(15 * 60)
    {
        return "false".into();
    }

    match heating_state.heating_is_on && heating_state.current_temp < heating_state.target_temp {
        true => "true".into(),
        false => "false".into(),
    }
}

async fn receive_temp(
    State(heating_state): State<Arc<Mutex<HeatingState>>>,
    Path(temp): Path<f64>,
) {
    let mut heating_state = heating_state.lock().await;
    heating_state.current_temp = temp;
    heating_state.current_temp_reported_at = SystemTime::now();
}
