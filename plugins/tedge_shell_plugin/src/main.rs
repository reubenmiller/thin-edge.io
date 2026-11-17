#[cfg(unix)]
use clap::Parser;
#[cfg(unix)]
use tedge_config::TEdgeConfig;
#[cfg(unix)]
use tedge_shell_plugin::bin::ShellCli;
#[cfg(unix)]
use tedge_shell_plugin::bin::TEdgeConfigView;

#[cfg(unix)]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = ShellCli::parse();

    let tedge_config = TEdgeConfig::load(&cli.common.config_dir).await?;
    let config = TEdgeConfigView::new(&tedge_config);

    tedge_shell_plugin::bin::run(cli, config).await
}

#[cfg(not(unix))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("The shell_execute operation is not supported on this platform")
}
