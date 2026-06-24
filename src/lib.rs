use axum::{Router, extract::{Form, Request, State}, http::StatusCode, routing::{get, post}};
use tokio::net::TcpListener;
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use uuid::Uuid;
use chrono::Utc;
pub mod configuration;
pub mod telemetry;

#[derive(serde::Deserialize)]
struct FormData {
    name: String,
    email: String
}

#[tracing::instrument(
    name = "Adding a new subscriber",
    skip(form,pool),
    fields(
        subscriber_email = %&form.email,
        subscriber_name = %&form.name
    )
)]
async fn subscribe(State(pool):State<PgPool>,Form(form): Form<FormData>) -> StatusCode {

    // tracing::info!("Received a new subscription request");
    // tracing::info!("Adding subscriber: name={}, email={}", form.name, form.email);


    //This way (the request_span.enter() only works for synchronous code. Leaving it commented out for reference). The correct way is tracing::instrument above.
    // let request_id = Uuid::new_v4();

    // let request_span = tracing::info_span!(
    //     "Adding a new subscriber",
    //     %request_id,
    //     subscriber_email = %&form.email,
    //     subscriber_name = %&form.name
    // );

    // let _request_span_guard = request_span.enter();

    match sqlx::query!(
        r#"
        INSERT INTO subscriptions (id,name,email,subscribed_at)
        VALUES ($1, $2, $3, $4)
        "#,
        Uuid::new_v4(),
        form.name,
        form.email,
        Utc::now()
    )
    .execute(&pool)
    .await
    {
        Ok(_) => {
            tracing::info!("Subscription succeeded!");
            StatusCode::OK
        },
        Err(e) => {
            tracing::error!("Failed to execute query: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }   
    }
} 

async fn health_check() -> StatusCode {
    StatusCode::OK
}

pub fn app(pool:PgPool) -> Router {
    Router::new()
    .route("/health_check", get(health_check))
    .route("/subscriptions",post(subscribe))
    //middleware
    // make_span_with(closure) runs once per incoming request and returns the span that wraps it.
    .layer(
        TraceLayer::new_for_http().make_span_with(|request: &Request<_>| {
            tracing::info_span!(
                "request",
                request_id = %Uuid::new_v4(),
                method = %request.method(),
                uri = %request.uri()
            )
        })
    )
    .with_state(pool)
}

pub async fn run(listener:TcpListener, pool:PgPool) -> Result<(),std::io::Error> {
    axum::serve(listener, app(pool)).await
}