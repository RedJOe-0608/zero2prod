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
    let listner = TcpListener::bind(format!("{}:{}",configuration.application.host,configuration.application.port)).await?;

    /*
    NOTE ON Wildcard IP address: 0.0.0.0 is a special IP. its saying, "hey OS, across all the IP addresses that this machine is assigned to, all the interfaces, (meaning, all the networks this machine is a prt of), any incoming request to port 8000 deliver it to me."
    
    - Important thing to understand is that, a machine can have multiple IPs. the loopback address, the wifi address, the ethernet one. essentially, IP address is given to locate the machine in a speicific network. if the machine is part of multiple networks, it has multiple IPs.
    
    and this is needed for docker containers. each container has its own networking. so, we do port forwarding, something like -p 8000:8000. So any incoming request on the machine's port 8000, it is delivered to port 8000 of the container. 
     */


    run(listner,pool).await
}

