use sc_cli::RunCmd;
use std::str::FromStr;

#[derive(Debug, clap::Subcommand)]
pub enum Subcommand {
	/// Key management cli utilities
	#[clap(subcommand)]
	Key(sc_cli::KeySubcommand),

	/// Build a chain specification.
	BuildSpec(sc_cli::BuildSpecCmd),

	/// Validate blocks.
	CheckBlock(sc_cli::CheckBlockCmd),

	/// Export blocks.
	ExportBlocks(sc_cli::ExportBlocksCmd),

	/// Export the state of a given block into a chain spec.
	ExportState(sc_cli::ExportStateCmd),

	/// Import blocks.
	ImportBlocks(sc_cli::ImportBlocksCmd),

	/// Remove the whole chain.
	PurgeChain(sc_cli::PurgeChainCmd),

	/// Revert the chain to a previous state.
	Revert(sc_cli::RevertCmd),

	/// The custom benchmark subcommand benchmarking runtime pallets.
	#[clap(subcommand)]
	/// The custom benchmark subcommmand benchmarking runtime pallets.
	Benchmark(frame_benchmarking_cli::BenchmarkCmd),

}

#[derive(Debug, Eq, PartialEq)]
pub enum RandomxFlag {
	LargePages,
	Secure,
}

impl FromStr for RandomxFlag {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"large-pages" => Ok(Self::LargePages),
			"secure" => Ok(Self::Secure),
			_ => Err("Unknown flag".to_string()),
		}
	}
}

#[derive(Debug, clap::Parser)]
pub struct Cli {
	#[structopt(subcommand)]
	pub subcommand: Option<Subcommand>,

	#[structopt(flatten)]
	pub run: RunCmd,

	#[structopt(long)]
	pub author: Option<String>,
	#[structopt(long)]
	pub threads: Option<usize>,
	#[structopt(long)]
	pub round: Option<u32>,
	#[structopt(long)]
	pub enable_polkadot_telemetry: bool,
	#[structopt(long)]
	pub disable_weak_subjectivity: bool,
	#[structopt(long)]
	pub check_inherents_after: Option<u32>,
	#[structopt(long)]
	pub randomx_flags: Vec<RandomxFlag>,
}
