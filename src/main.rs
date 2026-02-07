use std::fs;
use std::path::PathBuf;

use bash2amber::convert_bash_to_amber;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "bash2amber")]
#[command(about = "Convert Bash scripts into Amber code")]
struct Cli {
    #[arg(value_name = "INPUT")]
    input: PathBuf,

    #[arg(value_name = "OUTPUT")]
    output: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    let source = match fs::read_to_string(&cli.input) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("Failed to read '{}': {err}", cli.input.display());
            std::process::exit(1);
        }
    };

    let amber = match convert_bash_to_amber(&source, Some(cli.input.display().to_string())) {
        Ok(amber) => amber,
        Err(err) => {
            eprintln!("Failed to convert '{}': {err}", cli.input.display());
            std::process::exit(1);
        }
    };

    if let Some(output_path) = cli.output {
        if let Err(err) = fs::write(&output_path, &amber) {
            eprintln!("Failed to write '{}': {err}", output_path.display());
            std::process::exit(1);
        }
        return;
    }

    print!("{amber}");
}
