use clap::Parser;
use stegano::cli::{exit_code, Cli, Commands};

// wasm32 has no multi-threaded runtime; the CLI is only built there by
// `cargo check`, so a current-thread runtime is enough.
#[cfg_attr(not(target_arch = "wasm32"), tokio::main)]
#[cfg_attr(target_arch = "wasm32", tokio::main(flavor = "current_thread"))]
async fn main() {
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Embed {
            input,
            payload,
            output,
            password,
            validate,
            gpu,
            stealth,
            ec_level,
            strategy,
            cipher,
        } => {
            if let Err(e) = stegano::cli::commands::handle_embed(
                &input,
                &payload,
                &output,
                &password,
                validate,
                gpu,
                &stealth,
                &ec_level,
                strategy.as_deref(),
                &cipher,
            )
            .await
            {
                eprintln!("❌ Embedding failed: {e}");
                std::process::exit(exit_code::FAILURE);
            }
        }
        Commands::Extract {
            input,
            output,
            password,
            gpu,
            ec_level,
            strategy,
        } => {
            if let Err(e) = stegano::cli::commands::handle_extract(
                &input,
                &output,
                &password,
                gpu,
                &ec_level,
                strategy.as_deref(),
            )
            .await
            {
                eprintln!("❌ Extraction failed: {e}");
                std::process::exit(exit_code::FAILURE);
            }
        }
        Commands::Analyze { input, validate } => {
            if let Err(e) = stegano::cli::commands::handle_analyze(&input, validate).await {
                eprintln!("❌ Analysis failed: {e}");
                std::process::exit(exit_code::FAILURE);
            }
        }
        Commands::Detect { input, validate } => {
            // 0 = hidden data found, 1 = not found, 3 = error (2 is clap's usage error).
            let code = match stegano::cli::commands::handle_detect(&input, validate).await {
                Ok(true) => exit_code::SUCCESS,
                Ok(false) => exit_code::FAILURE,
                Err(e) => {
                    eprintln!("❌ Detection failed: {e}");
                    exit_code::DETECT_ERROR
                }
            };
            std::process::exit(code);
        }
    }
}
