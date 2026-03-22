use std::process;

#[tokio::main]
async fn main() {
    #[cfg(feature = "tailscale")]
    {
        eprintln!("tailscale listener not yet implemented");
        process::exit(1);
    }

    #[cfg(not(feature = "tailscale"))]
    {
        let addr = std::env::var("RAILSCALE_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
        let listener = match rail_proto::carriage::nontailscale::DevListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("failed to bind {addr}: {e}");
                process::exit(1);
            }
        };
        if let Err(e) = listener.run().await {
            eprintln!("server error: {e}");
            process::exit(1);
        }
    }
}
