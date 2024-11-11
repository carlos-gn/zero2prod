use secrecy::SecretString;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use std::sync::LazyLock;
use uuid::Uuid;
use zero2prod::configuration::{get_configuration, DatabaseSettings};
use zero2prod::startup::{get_connection_pool, Appplication};
use zero2prod::telemetry::{get_subscriber, init_subscriber};

pub struct TestApp {
    pub address: String,
    pub db_pool: PgPool,
    pub client: reqwest::Client,
}

impl TestApp {
    pub async fn post_subscriptions(&self, body: &'static str) -> reqwest::Response {
        self.client
            .post(&format!("{}/subscriptions", &self.address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to execute request.")
    }
}

static TRACING: LazyLock<()> = LazyLock::new(|| {
    let subscriber = get_subscriber("test".into(), "debug".into());
    init_subscriber(subscriber);
});

pub async fn spawn_app() -> TestApp {
    LazyLock::force(&TRACING);

    let configuration = {
        let mut c = get_configuration().expect("Failed to read configuration");
        c.database.database_name = Uuid::new_v4().to_string();
        c.application.port = 0;
        c
    };

    configure_database(&configuration.database).await;

    let application = Appplication::build(configuration.clone())
        .await
        .expect("Failed to build application");

    let address = format!("http://127.0.0.1:{}", application.port());
    tokio::spawn(application.run_until_stopped());

    let client = reqwest::Client::new();

    TestApp {
        address,
        client,
        db_pool: get_connection_pool(&configuration.database),
    }
}

pub async fn configure_database(config: &DatabaseSettings) -> PgPool {
    let maintenance_settings = DatabaseSettings {
        database_name: "postgres".to_string(),
        username: "postgres".to_string(),
        password: SecretString::from("password"),
        ..config.clone()
    };
    let mut connection = PgConnection::connect_with(&maintenance_settings.connect_options())
        .await
        .expect("failed to connect to Postgres");

    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, config.database_name).as_str())
        .await
        .expect("failed to create database.");

    let connection_pool = PgPool::connect_with(config.connect_options())
        .await
        .expect("failed to connect to Postgres.");

    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("failed to migrate database");

    connection_pool
}
