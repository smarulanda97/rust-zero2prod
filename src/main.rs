use std::{io, net::TcpListener};
use sqlx::postgres;
use zero2prod::{configuration, startup, telemetry};

#[tokio::main]
async fn main() -> io::Result<()> {
    let subscriber = telemetry::get_subscriber("zero2prod".into(), "info".into(), io::stdout);
    telemetry::init_subscriber(subscriber);

    let config = configuration::get_config().expect("Failed to read the config file");
    let connection_pool = postgres::PgPoolOptions::new()
        .connect_lazy_with(config.database.with_db());

    // Starting a new listener for the webserver
    let app_address = format!("{}:{}", config.application.host, config.application.port);
    let app_listener = TcpListener::bind(app_address).expect("Failed to bind address");

    startup::run(app_listener, connection_pool)?.await
}
