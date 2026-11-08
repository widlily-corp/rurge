//! Binary entrypoint for the Rurge Horizon native client.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    rurge_client::engine_host::launch()
}
