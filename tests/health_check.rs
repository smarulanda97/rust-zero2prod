use reqwest;
use once_cell::sync;
use std::{env, io, net};
use sqlx::{self, Executor, Connection};
use zero2prod::configuration::DatabaseSettings;
use zero2prod::{configuration, startup, telemetry};

static TRACING: sync::Lazy<()> = sync::Lazy::new(|| {
    let channel = "test".to_string();
    let default_level = "info".to_string();

    if env::var("TEST_LOG").is_ok() {
        let subscriber = telemetry::get_subscriber(channel, default_level, io::stdout);
        telemetry::init_subscriber(subscriber);
    } else {
        let subscriber = telemetry::get_subscriber(channel, default_level, io::sink);
        telemetry::init_subscriber(subscriber);
    }
});

pub struct TestApp {
    pub app_address: String,
    pub database_pool: sqlx::PgPool,
    pub http_client: reqwest::Client,
}

async fn spawn_test_database_pool(config: &DatabaseSettings) -> sqlx::PgPool {
    let mut connection = sqlx::PgConnection::connect_with(&config.without_db())
        .await
        .expect("Failed to connect to Postgres")
        .execute(format!(r#"CREATE DATABSE "{}"; "#, config.name).as_str())
        .await
        .expect("Failed to create the test database");

    let connection_pool = sqlx::PgPool::connect_with(config.with_db())
        .await
        .expect("Failed to connect to Postgres");

    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate the database");

    connection_pool
}

async fn spawn_test_app() -> TestApp {
    sync::Lazy::force(&TRACING);

    let config = configuration::get_config().expect("Failed to read the configuration.yaml");
    let app_listener = net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind a random port");
    let app_port = app_listener.local_addr().unwrap().port();
    let app_address = format!("http://127.0.0.1:{}", app_port);

    let database_pool = spawn_test_database_pool(&config.database).await;
    let app_server =
        startup::run(app_listener, database_pool.clone()).expect("Failed to bind the address");

    let _ = tokio::spawn(app_server);

    TestApp {
        app_address,
        database_pool,
        http_client: reqwest::Client::new(),
    }
}

#[tokio::test]
async fn health_check_works() {
    let test_app = spawn_test_app().await;

    let response = test_app
        .http_client
        .get(&format!("{}/health_check", test_app.app_address))
        .send()
        .await
        .expect("Failed to execute request");

    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}

#[tokio::test]
async fn subscribe_returns_a_200_for_valid_form_data() {
    let test_app = spawn_test_app().await;

    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";
    let response = test_app
        .http_client
        .post(&format!("{}/subscriptions", test_app.app_address))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("Failed to execute the request");

    assert_eq!(200, response.status().as_u16());

    let saved = sqlx::query!("SELECT email, name FROM subscriptions",)
        .fetch_one(&test_app.database_pool)
        .await
        .expect("Failed to fetch saved subscription");

    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
}

#[tokio::test]
async fn subscribe_returns_a_400_when_data_is_missing() {
    let test_app = spawn_test_app().await;
    let test_cases = vec![
        ("name=le%20guin", "missing the email"),
        ("email=ursula_le_guin_%40gmail.com", "missing the name"),
        ("", "missing both name, and email"),
    ];

    for (invalid_body, error_message) in test_cases {
        let response = test_app
            .http_client
            .post(&format!("{}/subscriptions", &test_app.app_address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(invalid_body)
            .send()
            .await
            .expect("Failed to execute the request");

        assert_eq!(
            400,
            response.status().as_u16(),
            "The API didn't fail with 400 Bad Request when the payload was {}",
            error_message
        );
    }
}
