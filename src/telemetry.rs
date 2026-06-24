use tracing::{Subscriber, subscriber::set_global_default};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_log::LogTracer;
use tracing_subscriber::{EnvFilter, Registry, fmt::MakeWriter, layer::SubscriberExt};


// a subscriber is the one component that receives every tracing event in our program.
// the subscriber answers 3 things:
// 1. Keep or drop it? (is this event loud enough — info? debug?)
// 2. Format it how? (plain text? JSON?)
// 3. Send it where? (terminal? file? nowhere?)

// The app writes to stdout (dumb, portable). The platform routes that stream wherever it should go (durable storage)
pub fn get_subscriber<Sink>(name: String, env_filter: String,sink:Sink) -> impl Subscriber + Send + Sync 
where Sink: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(env_filter));

    let formatting_layer = BunyanFormattingLayer::new(name,sink);

    Registry::default()
        .with(env_filter)
        .with(JsonStorageLayer)
        .with(formatting_layer)
}

pub fn init_subscriber(subscriber: impl Subscriber + Send + Sync) {
    LogTracer::init().expect("Failed to set logger");

    set_global_default(subscriber).expect("Failed to set subscriber");
}