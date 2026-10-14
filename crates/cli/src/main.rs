//! `native-shell`: create, check, type, sign and serve native-shell programs.

mod check;
mod dev;
mod program;
mod scaffold;
mod sign;
mod types;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "native-shell", version, about = "Create, check, sign and serve native-shell programs")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new program (manifest, main.js, editor and agent setup).
    New {
        dir: PathBuf,
        /// Host registry JSON (modules and components) for the generated types.
        #[arg(long)]
        registry: Option<PathBuf>,
    },
    /// Load and render a program headlessly; exit 1 on any error.
    Check {
        dir: PathBuf,
        /// Print the native view tree after the first render.
        #[arg(long)]
        print_tree: bool,
        /// Host registry JSON (modules and components the app provides).
        #[arg(long)]
        registry: Option<PathBuf>,
    },
    /// Write native-shell.d.ts into the program directory.
    Types {
        dir: PathBuf,
        #[arg(long)]
        registry: Option<PathBuf>,
    },
    /// Generate an Ed25519 signing key pair: <prefix>.secret and <prefix>.pub (base64).
    Keygen {
        #[arg(long)]
        out: PathBuf,
    },
    /// Build a signed .nspkg bundle.
    Bundle {
        dir: PathBuf,
        /// Secret key: a file written by `keygen`, or the base64 key itself.
        #[arg(long)]
        key: String,
        /// Key id the runtime trusts the public key under.
        #[arg(long)]
        key_id: String,
        #[arg(short, long)]
        output: PathBuf,
        /// Ship sources only (no precompiled QuickJS bytecode).
        #[arg(long)]
        no_bytecode: bool,
    },
    /// Verify a bundle's signature and integrity.
    Verify {
        file: PathBuf,
        /// Public key: a .pub file or the base64 key itself.
        #[arg(long)]
        trusted: String,
    },
    /// Serve a program to devices on the LAN with hot reload and remote logs.
    Dev {
        dir: PathBuf,
        #[arg(long, default_value_t = 8737)]
        port: u16,
        /// Host registry JSON, for checking each version before devices load it.
        #[arg(long)]
        registry: Option<PathBuf>,
    },
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::New { dir, registry } => {
            let registry = program::read_registry(registry.as_deref())?;
            scaffold::new_program(&dir, &registry)?;
            println!("Created {}. Next: native-shell check {}", dir.display(), dir.display());
        }
        Command::Check { dir, print_tree, registry } => {
            let registry = program::read_registry(registry.as_deref())?;
            return check::run(&dir, &registry, print_tree);
        }
        Command::Types { dir, registry } => {
            let registry = program::read_registry(registry.as_deref())?;
            let path = dir.join("native-shell.d.ts");
            std::fs::write(&path, types::generate(&registry))?;
            println!("Wrote {}", path.display());
        }
        Command::Keygen { out } => {
            let (secret, public) = sign::keygen(&out)?;
            println!("Wrote {} (keep it secret) and {}", secret.display(), public.display());
        }
        Command::Bundle { dir, key, key_id, output, no_bytecode } => {
            let bytes = sign::build(&dir, &key, &key_id, &output, !no_bytecode)?;
            println!("Wrote {} ({} bytes)", output.display(), bytes.len());
        }
        Command::Verify { file, trusted } => {
            println!("{}", sign::verify(&file, &trusted)?);
        }
        Command::Dev { dir, port, registry } => {
            let registry = program::read_registry(registry.as_deref())?;
            let check_dir = dir.clone();
            let server = dev::DevServer::start(&dir, port, move |version, error| {
                if let Some(e) = error {
                    eprintln!("[dev] error: {e}");
                    return;
                }
                println!("[dev] serving version {version}");
                // Devices load it either way; the check tells the developer first.
                match check::check(&check_dir, &registry) {
                    Ok((_, report)) => report.errors.iter().for_each(|e| eprintln!("[dev] error: {e}")),
                    Err(e) => eprintln!("[dev] error: {e:#}"),
                }
            })?;
            let url = format!("http://{}:{}", dev::lan_ip(), server.addr.port());
            println!("Serving {} at {url}", dir.display());
            println!("Run it on Linux (in platforms/linux): cargo run --example run -- --dev {url}");
            loop {
                std::thread::park();
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
