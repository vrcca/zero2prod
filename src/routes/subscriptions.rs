use actix_web::{HttpResponse, Responder, web};
use chrono::Utc;
use sqlx::PgPool;
use tracing::Instrument;
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

pub async fn subscribe(form: web::Form<FormData>, pg_pool: web::Data<PgPool>) -> impl Responder {
    let request_id = Uuid::new_v4();

    let request_span = tracing::info_span!("Adding a new subscriber.",
%request_id, subscriber_email = %form.email, subscriber_name = %form.name);

    let _request_span_guard = request_span.enter();

    let query_span = tracing::info_span!("Saving new subscriber details in the database");
    let result = sqlx::query!(
        r#"
        INSERT INTO subscriptions(id, email, name, subscribed_at)
        VALUES ($1, $2, $3, $4)
        "#,
        Uuid::new_v4(),
        form.email,
        form.name,
        Utc::now()
    )
    .execute(pg_pool.get_ref())
    .instrument(query_span)
    .await;

    match result {
        Ok(_) => {
            tracing::info!("New subscriber details have been saved");
            HttpResponse::Ok()
        }
        Err(e) => {
            tracing::error!("Failed to execute query: {:?}", e);
            HttpResponse::InternalServerError()
        }
    }
}
