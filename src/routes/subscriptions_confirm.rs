use actix_web::{HttpResponse, Responder, web};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct Parameters {
    subscription_token: String,
}

#[tracing::instrument(name = "Confirm a pending subscriber", skip(parameters, pg_pool))]
pub async fn confirm(
    parameters: web::Query<Parameters>,
    pg_pool: web::Data<PgPool>,
) -> impl Responder {
    match fetch_subscriber_id_from_token(&pg_pool, &parameters.subscription_token).await {
        Ok(None) => return HttpResponse::Unauthorized(),
        Err(_) => return HttpResponse::InternalServerError(),
        Ok(Some(subscriber_id)) => {
            if confirm_subscriber(&pg_pool, subscriber_id).await.is_err() {
                return HttpResponse::InternalServerError();
            }
        }
    };

    HttpResponse::Ok()
}

#[tracing::instrument(
    name = "Uses subscription token, returning subscriber",
    skip(pool, token)
)]
async fn fetch_subscriber_id_from_token(
    pool: &PgPool,
    token: &String,
) -> Result<Option<Uuid>, sqlx::Error> {
    let record = sqlx::query!(
        "DELETE FROM subscription_tokens WHERE subscription_token = $1 RETURNING subscriber_id;",
        token
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;

    Ok(record.map(|r| r.subscriber_id))
}

#[tracing::instrument(name = "Mark subscriber as confirmed", skip(pool))]
async fn confirm_subscriber(pool: &PgPool, subscriber_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE subscriptions SET status = $1 WHERE id = $2",
        "confirmed",
        subscriber_id
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;

    Ok(())
}
