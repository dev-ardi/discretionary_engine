pub mod api;
pub mod config;
pub mod positions;
pub mod protocols;
use clap::{Args, Parser, Subcommand};
use config::Config;
use positions::*;
use v_utils::{
	io::ExpandedPath,
	trades::{Side, Timeframe},
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
	#[command(subcommand)]
	command: Commands,
	#[arg(long, default_value = "~/.config/discretionary_engine.toml")]
	config: ExpandedPath,
	#[arg(short, long, action = clap::ArgAction::SetTrue)]
	noconfirm: bool,
}
#[derive(Subcommand)]
enum Commands {
	/// Start the program
	New(PositionArgs),
}
#[derive(Args)]
struct PositionArgs {
	/// percentage of the total balance to use
	#[arg(long)]
	size: f64,
	/// timeframe, in the format of "1m", "1h", "3M", etc.
	/// determines the target period for which we expect the edge to persist.
	#[arg(long)]
	tf: Option<Timeframe>,
	/// _only_ the coin name itself. e.g. "BTC" or "ETH". Providing full symbol currently will lead to undefined behavior.
	#[arg(long)]
	coin: String,
	/// position acquisition parameters, in the format of "<protocol>-<params>", e.g. "ts:p0.5". Params consist of their starting letter followed by the value, e.g. "p0.5" for 0.5% offset. If multiple params are required, they are separated by '-'.
	#[arg(short, long, default_value = "")]
	acquisition_protocols_spec: Vec<String>,
	/// position followup parameters, in the format of "<protocol>-<params>", e.g. "ts:p0.5". Params consist of their starting letter followed by the value, e.g. "p0.5" for 0.5% offset. If multiple params are required, they are separated by '-'.
	#[arg(short, long, default_value = "")]
	followup_protocols_spec: Vec<String>,
}

// Later on we will initialize exchange sockets once, then just have a loop listening on localhost, that accepts new positions or modification requests.

#[tokio::main]
async fn main() {
	let cli = Cli::parse();
	let config = match Config::try_from(cli.config) {
		Ok(cfg) => cfg,
		Err(e) => {
			eprintln!("Loading config failed: {}", e);
			std::process::exit(1);
		}
	};
	let noconfirm = cli.noconfirm;

	match cli.command {
		Commands::New(position_args) => {
			// init position
			// update acquisition and followup protocols on it
			// they themselves decide whether cache needs to be updated/created

			let balance = api::compile_total_balance(config.clone()).await.unwrap();
			let (side, target_size) = match position_args.size {
				s if s > 0.0 => (Side::Buy, s * balance),
				s if s < 0.0 => (Side::Sell, -s * balance),
				_ => {
					eprintln!("Size must be non-zero");
					std::process::exit(1);
				}
			};

			let spec = PositionSpec::new(position_args.coin, side, target_size);
			let acquired = PositionAcquisition::do_acquisition(spec).await.unwrap();
			let closed = PositionFollowup::do_followup(acquired).await.unwrap();

			//let protocols = ProtocolsSpec::try_from(position_args.followup_protocols_spec).unwrap();
			//
			//let cache = FollowupCache::new();
		}
	}
}
