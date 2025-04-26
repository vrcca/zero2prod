use std::net::TcpListener;

use actix_web::{App, HttpServer, dev::Server, middleware::Logger, web};
use sqlx::PgPool;

use crate::routes::{health_check, subscribe};

pub fn run(listener: TcpListener, db_pool: PgPool) -> Result<Server, std::io::Error> {
    let db_pool = web::Data::new(db_pool);
    let app = move || {
        App::new()
            .wrap(Logger::default())
            .route("/health_check", web::get().to(health_check))
            .route("/subscriptions", web::post().to(subscribe))
            .app_data(db_pool.clone())
    };
    let server = HttpServer::new(app).listen(listener)?.run();
    Ok(server)
}
