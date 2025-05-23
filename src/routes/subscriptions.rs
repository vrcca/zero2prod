use actix_web::{
    HttpResponse, Responder,
    web::{self, Data},
};
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
) -> impl Responder {
    let Ok(new_subscriber) = form.0.try_into() else {
        return HttpResponse::BadRequest();
    };
    let mut transaction = match pg_pool.begin().await {
        Ok(transaction) => transaction,
        Err(_) => return HttpResponse::InternalServerError(),
    };

    let Ok(suscriber_id) = insert_subscriber(&mut *transaction, &new_subscriber).await else {
        return HttpResponse::InternalServerError();
    };

    let subscription_token = generate_subscription_token();

    if store_token(&mut *transaction, suscriber_id, &subscription_token)
        .await
        .is_err()
    {
        return HttpResponse::InternalServerError();
    }
    if transaction.commit().await.is_err() {
        return HttpResponse::InternalServerError();
    }

    if send_confirmation_email(
        &email_client,
        new_subscriber,
        &base_url.0,
        &subscription_token,
    )
    .await
    .is_err()
    {
        return HttpResponse::InternalServerError();
    }
    HttpResponse::Ok()
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
) -> Result<(), sqlx::Error> {
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
        e
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
