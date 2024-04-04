use once_cell::sync::Lazy;
use reqwest::Client;
use secrecy::ExposeSecret;
use sqlx::PgPool;
use std::{env, io, net::TcpListener};
use zero2prod::{configuration, startup, telemetry};

static TRACING: Lazy<()> = Lazy::new(|| {
    let default_filter_level = "info".to_string();
    let subscriber_name = "test".to_string();

    if env::var("TEST_LOG").is_ok() {
        let subscriber =
            telemetry::get_subscriber(subscriber_name, default_filter_level, io::stdout);
        telemetry::init_subscriber(subscriber);
    } else {
        let subscriber = telemetry::get_subscriber(subscriber_name, default_filter_level, io::sink);
        telemetry::init_subscriber(subscriber);
    }
});

/// TestApp
///
/// Struct that contains the necessary elements to use in the tests:
///
/// - db_connection:
/// - address:
/// - http_client:
///
pub struct TestApp {
    pub db_connection: PgPool,
    pub address: String,
    pub http_client: Client,
}

async fn spawn_db_conn() -> PgPool {
    let config = configuration::get_config().expect("Failed to read the configuration.yaml");

    PgPool::connect(&config.database.get_conn_string().expose_secret())
        .await
        .expect("Failed to start the db pool")
}

async fn spawn_app() -> TestApp {
    Lazy::force(&TRACING);

    // Find a port for loading the app in test mode
    let app_listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind a random port");
    let app_port = app_listener.local_addr().unwrap().port();
    let address = format!("http://127.0.0.1:{}", app_port);
    // Load the database connection
    let db_connection = spawn_db_conn().await;
    // Star the web serve
    let app_server =
        startup::run(app_listener, db_connection.clone()).expect("Failed to bind the address");
    // Create a http client
    let http_client = Client::new();

    let _ = tokio::spawn(app_server);

    TestApp {
        address,
        http_client,
        db_connection,
    }
}

#[tokio::test]
async fn health_check_works() {
    let test_app = spawn_app().await;

    let response = test_app
        .http_client
        .get(&format!("{}/health_check", test_app.address))
        .send()
        .await
        .expect("Failed to execute request");

    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}

#[tokio::test]
async fn subscribe_returns_a_200_for_valid_form_data() {
    let test_app = spawn_app().await;

    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";
    let response = test_app
        .http_client
        .post(&format!("{}/subscriptions", test_app.address))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("Failed to execute the request");

    assert_eq!(200, response.status().as_u16());

    let saved = sqlx::query!("SELECT email, name FROM subscriptions",)
        .fetch_one(&test_app.db_connection)
        .await
        .expect("Failed to fetch saved subscription");

    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
}

#[tokio::test]
async fn subscribe_returns_a_400_when_data_is_missing() {
    let test_app = spawn_app().await;
    let test_cases = vec![
        ("name=le%20guin", "missing the email"),
        ("email=ursula_le_guin_%40gmail.com", "missing the name"),
        ("", "missing both name, and email"),
    ];

    for (invalid_body, error_message) in test_cases {
        let response = test_app
            .http_client
            .post(&format!("{}/subscriptions", &test_app.address))
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
