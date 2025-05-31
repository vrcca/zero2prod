use actix_web::{
    HttpResponse, Responder, ResponseError,
    http::StatusCode,
    web::{self, Data},
};
use anyhow::Context as _;
use chrono::Utc;
use rand::distr::{Alphanumeric, SampleString};
use sqlx::{PgConnection, PgPool};
use tera::Context;
use uuid::Uuid;

use crate::{
    configuration::email_templates,
    domain::{NewSubscriber, SubscriberEmail, SubscriberName},
    email_client::EmailClient,
    startup::ApplicationBaseUrl,
};

pub struct StoreTokenError(sqlx::Error);
impl std::fmt::Display for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "A database error was encountered while \
        trying to store a subscription token."
        )
    }
}
impl std::error::Error for StoreTokenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}
impl std::fmt::Debug for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

impl TryFrom<FormData> for NewSubscriber {
    type Error = String;

    fn try_from(value: FormData) -> Result<Self, Self::Error> {
        let name = SubscriberName::parse(value.name)?;
        let email = SubscriberEmail::parse(value.email)?;
        Ok(Self { name, email })
    }
}

#[tracing::instrument(
    name = "Adding a new subscriber",
    skip(form, pg_pool, email_client, base_url),
    fields(
        subscriber_email = %form.email,
        subscriber_name = %form.name
    )
)]

pub async fn subscribe(
    form: web::Form<FormData>,
    pg_pool: Data<PgPool>,
    email_client: Data<EmailClient>,
    base_url: Data<ApplicationBaseUrl>,
) -> Result<impl Responder, SubscribeError> {
    let new_subscriber = form.0.try_into().map_err(SubscribeError::ValidationError)?;
    let mut transaction = pg_pool
        .begin()
        .await
        .context("Failed to acquire a Postgres connection from the pool")?;
    let suscriber_id = insert_subscriber(&mut *transaction, &new_subscriber)
        .await
        .context("Failed to insert new subscriber in the database.")?;
    let subscription_token = generate_subscription_token();
    store_token(&mut *transaction, suscriber_id, &subscription_token)
        .await
        .context("Failed to store the confirmation token for a new subscriber.")?;
    transaction
        .commit()
        .await
        .context("Failed to commit SQL transaction to store a new subscriber.")?;

    send_confirmation_email(
        &email_client,
        new_subscriber,
        &base_url.0,
        &subscription_token,
    )
    .await
    .context("Failed to send a confirmation email.")?;
    Ok(HttpResponse::Ok())
}

#[derive(thiserror::Error)]
pub enum SubscribeError {
    #[error("{0}")]
    ValidationError(String),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for SubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for SubscribeError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        match self {
            Self::ValidationError(_) => StatusCode::BAD_REQUEST,
            Self::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(new_subscriber, transaction)
)]
pub async fn insert_subscriber(
    transaction: &mut PgConnection,
    new_subscriber: &NewSubscriber,
) -> Result<Uuid, sqlx::Error> {
    let subscriber_id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO subscriptions(id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, $4, 'pending_confirmation');
        "#,
        subscriber_id,
        new_subscriber.email.as_ref(),
        new_subscriber.name.as_ref(),
        Utc::now()
    )
    .execute(&mut *transaction)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;

    Ok(subscriber_id)
}

fn generate_subscription_token() -> String {
    Alphanumeric.sample_string(&mut rand::rng(), 25)
}

#[tracing::instrument(
    name = "Storing subscription token",
    skip(transaction, subscription_token)
)]
async fn store_token(
    transaction: &mut PgConnection,
    subscriber_id: Uuid,
    subscription_token: &String,
) -> Result<(), StoreTokenError> {
    sqlx::query!(
        r#"
        INSERT INTO subscription_tokens (subscriber_id, subscription_token) 
        VALUES ($1, $2);
        "#,
        subscriber_id,
        subscription_token,
    )
    .execute(&mut *transaction)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        StoreTokenError(e)
    })?;

    Ok(())
}

#[tracing::instrument(
    name = "Sending welcome notification to new subscriber",
    skip(email_client, new_subscriber)
)]
async fn send_confirmation_email(
    email_client: &EmailClient,
    new_subscriber: NewSubscriber,
    base_url: &str,
    subscription_token: &str,
) -> Result<(), reqwest::Error> {
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        base_url, subscription_token
    );
    let mut context = Context::new();
    context.insert("confirmation_link", &confirmation_link);
    let html_body = email_templates()
        .render("email_confirmation.html", &context)
        .expect("Unable to render email confirmation template!");

    let text_body = &format!(
        "Welcome to our newsletter!\n \
        Visit {} to confirm your subscription.",
        confirmation_link
    );
    email_client
        .send_email(new_subscriber.email, "Welcome!", &html_body, &text_body)
        .await
        .map_err(|e| {
            tracing::error!("Failed to send welcome message: {:?}", e);
            e
        })?;
    Ok(())
}

fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{}", cause)?;
        current = cause.source();
    }
    Ok(())
}
