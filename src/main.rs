use tokio::net::TcpListener;
use zero2prod::run;
use zero2prod::configuration::get_configuration;
use zero2prod::telemetry::{get_subscriber,init_subscriber};


// the main fn cannot be an async function. The OS needs a fn to call. so we use the attribute, which at compile time rewrites our async main fn into a normal main fn.
#[tokio::main]
async fn main() -> Result<(),std::io::Error>{

    // The fmt::init() emits logs as plain text. It is fairly human-redeable, but not machine-queryable. So, in production, we want our logs (whereever we might store them) so that the logs are searchable.
    // so we build our own telemetry 
    // tracing_subscriber::fmt::init();

    let subscriber = get_subscriber("zero2prod".into(), "info".into(),std::io::stdout);
    init_subscriber(subscriber);

    // load .env first
    dotenvy::dotenv().ok(); 

    let configuration = get_configuration().expect("failed to read configuration");

    let pool = sqlx::PgPool::connect_with(configuration.database.with_db())
    .await
    .expect("failed to connect to postres");

    // binding to a port can fail with an IO error
    let listner = TcpListener::bind("127.0.0.1:8000").await?;
    run(listner,pool).await
}

