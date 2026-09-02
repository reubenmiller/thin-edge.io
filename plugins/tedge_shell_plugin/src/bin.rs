use crate::execute;
use crate::write_launch_error;
use crate::write_script_output;
use camino::Utf8PathBuf;
use std::io::Write;
use tedge_config::cli::CommonArgs;
use tedge_config::log_init;
use tracing::error;
use tracing::info;

#[derive(clap::Parser, Debug)]
#[clap(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!(),
    arg_required_else_help(true)
)]
pub struct ShellCli {
    #[command(flatten)]
    pub common: CommonArgs,

    /// The command to be executed by the shell
    #[clap(long = "command")]
    command: String,

    /// The shell used to execute the command
    ///
    /// Defaults to the `shell.path` tedge configuration setting.
    #[clap(long = "shell")]
    shell: Option<Utf8PathBuf>,
}

/// The `tedge config` settings used by this plugin
#[derive(Debug)]
pub struct TEdgeConfigView {
    pub shell: Utf8PathBuf,
    pub tmp_dir: Utf8PathBuf,
    pub max_output_size: u32,
}

pub fn run(cli: ShellCli, config: TEdgeConfigView) -> anyhow::Result<()> {
    if let Err(err) = log_init(
        "tedge-shell-plugin",
        &cli.common.log_args,
        &cli.common.config_dir,
    ) {
        error!("Can't enable logging due to error: {err}");
        return Err(err.into());
    }

    let mut stdout = std::io::stdout();
    if cli.command.trim().is_empty() {
        // Reported using the script output protocol, so the user is told what is wrong
        // rather than only that this plugin returned a non-zero exit code
        let reason = "No command to execute";
        write_launch_error(&mut stdout, reason)?;
        stdout.flush()?;
        anyhow::bail!(reason);
    }

    let shell = cli.shell.unwrap_or(config.shell);
    info!("Executing command using shell: {shell}");

    let outcome = execute(
        &shell,
        &cli.command,
        &config.tmp_dir,
        config.max_output_size,
    );

    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(err) => {
            let reason = format!("Failed to run the command: {err}");
            write_launch_error(&mut stdout, &reason)?;
            stdout.flush()?;
            anyhow::bail!(reason);
        }
    };

    write_script_output(&mut stdout, &outcome)?;
    stdout.flush()?;

    if outcome.exit_code != 0 {
        info!(
            "Command returned a non-zero exit code. code={}",
            outcome.exit_code
        );
    }

    // Propagate the command exit code, so the workflow can tell success from failure
    std::process::exit(outcome.exit_code)
}
