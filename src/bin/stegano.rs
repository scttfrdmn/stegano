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
        } => {
            if let Err(e) =
                stegano::cli::commands::handle_embed(&input, &payload, &output, password.as_deref(), validate, gpu, &stealth)
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
        } => {
            if let Err(e) =
                stegano::cli::commands::handle_extract(&input, &output, password.as_deref(), gpu).await
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
    }

    Ok(())
}
