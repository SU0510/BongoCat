//! The InSync server binary. Configuration comes from the environment:
//! INSYNC_DB, INSYNC_PORT, INSYNC_TOKEN_TTL_DAYS (see docs/insync/deployment.md).

/// Startup and fatal errors go to stderr as single human-readable lines.
#[allow(clippy::print_stderr)]
fn report(line: &str) {
    eprintln!("{line}");
}

fn main() {
    let config = match insync_server::ServerConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            report(&format!("insync-server: invalid configuration: {error}"));
            std::process::exit(2);
        }
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    if let Err(error) = runtime.block_on(insync_server::serve(config)) {
        report(&format!("insync-server: {error}"));
        std::process::exit(1);
    }
}
