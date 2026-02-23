use clap::Parser;
use stegano::cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
                password.as_deref(),
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
                std::process::exit(1);
            }
        }
        Commands::Extract {
            input,
            output,
            password,
            gpu,
            strategy,
        } => {
            if let Err(e) = stegano::cli::commands::handle_extract(
                &input,
                &output,
                password.as_deref(),
                gpu,
                strategy.as_deref(),
            )
            .await
            {
                eprintln!("❌ Extraction failed: {e}");
                std::process::exit(1);
            }
        }
        Commands::Analyze { input, validate } => {
            if let Err(e) = stegano::cli::commands::handle_analyze(&input, validate).await {
                eprintln!("❌ Analysis failed: {e}");
                std::process::exit(1);
            }
        }
        Commands::Detect { input, validate } => {
            if let Err(e) = stegano::cli::commands::handle_detect(&input, validate).await {
                eprintln!("❌ Detection failed: {e}");
                std::process::exit(2);
            }
        }
    }

    Ok(())
}
