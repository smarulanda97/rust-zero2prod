use config::{Config, File};
use secrecy::{ExposeSecret, Secret};
use std::env;

/// # Environment
///
/// This enum holds the possible environment values for this
/// application (local or production).
pub enum Environment {
    Local,
    Production,
}

impl Environment {
    pub fn as_str(&self) -> &'static str {
        match self {
            Environment::Local => "local",
            Environment::Production => "production",
        }
    }
}

impl TryFrom<String> for Environment {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "production" => Ok(Self::Production),
            other => Err(format!(
                "{} is not a supported environment. \
                Use either `local` or `production`.",
                other
            )),
        }
    }
}

/// # Settings
///
/// Global object that holds the whole configuration for the
/// application and other services.
#[derive(serde::Deserialize)]
pub struct Settings {
    pub database: DatabaseSettings,
    pub application: ApplicationSettings,
}

/// ## DatabaseSettings
///
/// Object that holds the properties with connection
/// information for the database.
#[derive(serde::Deserialize)]
pub struct DatabaseSettings {
    pub user: String,
    pub password: Secret<String>,
    pub port: u16,
    pub host: String,
    pub name: String,
}

impl DatabaseSettings {
    pub fn get_conn_string(&self) -> Secret<String> {
        Secret::new(format!(
            "postgres://{}:{}@{}:{}/{}",
            self.user,
            self.password.expose_secret(),
            self.host,
            self.port,
            self.name
        ))
    }
}

/// ## ApplicationSettings
/// Object that holds the configuration of the application
///
#[derive(serde::Deserialize)]
pub struct ApplicationSettings {
    pub port: u16,
    pub host: String,
}

pub fn get_config() -> Result<Settings, config::ConfigError> {
    let root_dir = env::current_dir().expect("Failed to determine the current directory");
    let config_directory = root_dir.join("config");
    // Detect the running environment
    // Default to `local` if unspecified
    let environment: Environment = env::var("APP_ENVIRONMENT")
        .unwrap_or_else(|_| "local".into())
        .try_into()
        .expect("Failed to parse APP_ENVIRONMENT");
    let environment_filename = format!("{}.yaml", environment.as_str());

    let settings = Config::builder()
        .add_source(File::from(config_directory.join("base.yaml")))
        .add_source(File::from(config_directory.join(environment_filename)))
        .build()?;

    settings.try_deserialize::<Settings>()
}
