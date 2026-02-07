use std::fs;
use std::path::PathBuf;

use bash2amber::convert_bash_to_amber;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "bash2amber")]
#[command(about = "Convert Bash scripts into Amber code")]
struct Cli {
    #[arg(value_name = "INPUT", required = true)]
    input: Vec<PathBuf>,

    #[arg(short, long, value_name = "OUTPUT")]
    output: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    if cli.output.is_some() && cli.input.len() > 1 {
        eprintln!("--output can be used only with a single input file");
        std::process::exit(2);
    }

    let mut rendered = Vec::new();
    for path in &cli.input {
        let source = match fs::read_to_string(path) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("Failed to read '{}': {err}", path.display());
                std::process::exit(1);
            }
        };

        let amber = match convert_bash_to_amber(&source, Some(path.display().to_string())) {
            Ok(amber) => amber,
            Err(err) => {
                eprintln!("Failed to convert '{}': {err}", path.display());
                std::process::exit(1);
            }
        };

        rendered.push((path, amber));
    }

    if let Some(output_path) = cli.output {
        if let Err(err) = fs::write(&output_path, &rendered[0].1) {
            eprintln!("Failed to write '{}': {err}", output_path.display());
            std::process::exit(1);
        }
        return;
    }

    if rendered.len() == 1 {
        print!("{}", rendered[0].1);
        return;
    }

    for (index, (path, amber)) in rendered.into_iter().enumerate() {
        if index > 0 {
            println!();
        }
        println!("// Source: {}", path.display());
        print!("{amber}");
    }
}
