mod cli;
mod command;

fn main() -> sc_cli::Result<()> {
	command::run()
}
