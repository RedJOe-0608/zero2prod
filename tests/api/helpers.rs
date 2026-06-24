use std::sync::LazyLock;

use sqlx::{Connection, Executor, PgConnection, PgPool};
use uuid::Uuid;
use zero2prod::{configuration::{DatabaseSettings, get_configuration}, run, telemetry::{get_subscriber, init_subscriber}};
use tokio::net::TcpListener;

pub struct TestApp {
    pub address: String,
    pub db_pool: PgPool
}

// a static lives for the whole process and is shared by every thread. LazyLock is thread safe,  if two tests race to be the very first toucher, it internally synchronizes so the closure runs exactly once and the other waits.
static TRACING: LazyLock<()> = LazyLock::new(|| {
    // naming it test, so test logs are distinguishable.
    let subscriber_name = "test".to_string();
    let default_filter_level = "info".to_string();

    if std::env::var("TEST_LOG").is_ok() {
        let subscriber = get_subscriber(subscriber_name,default_filter_level,std::io::stdout);
        init_subscriber(subscriber); // () means "I return nothing." init_subscriber returns nothing. Its whole job is the side effect of filling the global slot.
    }else{
        let subscriber = get_subscriber(subscriber_name,default_filter_level,std::io::sink);
        init_subscriber(subscriber); // () means "I return nothing." init_subscriber returns nothing. Its whole job is the side effect of filling the global slot.
    }
});

/*
    We create a fresh, randomly-named DB for every test. The whole test suite
    (tests/api) compiles as ONE binary = one process, and the test harness runs
    the tests CONCURRENTLY on a thread pool. Concurrency alone is fine — the
    danger is concurrency + SHARED MUTABLE STATE (one common DB → UNIQUE(email)
    collisions, cross-test row bleed, flaky non-deterministic failures).
    The cheapest fix is to remove the sharing: give each test its own private
    database so there's nothing to collide over.
*/

/*
    We need to install the tracing once GLOBALLY, as all tests share the same global state.
*/


pub async fn spawn_app() -> TestApp {

    // We call the lazy lock closure here.
    LazyLock::force(&TRACING);

    dotenvy::dotenv().ok();

    let mut configuration = get_configuration().expect("Failed to load configuration");
    configuration.database.database_name = Uuid::new_v4().to_string();
    let pool = configure_database(&configuration.database).await;


    // Bind to port 0 -> OS picks a free port
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("Failed to bind random port");

    // Ask which port we actually got
    let port = listener.local_addr().unwrap().port();

    // Launch the server in the background:
    tokio::spawn(run(listener,pool.clone()));

    TestApp {
        address: format!("http://127.0.0.1:{port}"),
        db_pool: pool
    }
}

async fn configure_database(configuration: &DatabaseSettings) -> PgPool{
    //1. connect to the Postgres server (no specific dB) and create the new dB
    let mut connection = PgConnection::connect_with(&configuration.without_db())
    .await
    .expect("Failed to connect to postgres");

    let query = format!(r#"CREATE DATABASE "{}";"#, configuration.database_name);
    connection.execute(sqlx::raw_sql(sqlx::AssertSqlSafe(query)))
    .await
    .expect("Failed to create a dB");

    // 2. Connect a POOL to the new database and run migrations on it
    let pool = PgPool::connect_with(configuration.with_db())
        .await
        .expect("Failed to connect to Postgres.");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to migrate the database.");

    pool
}