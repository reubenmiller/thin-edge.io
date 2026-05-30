#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use anyhow::Context;
use cap::Cap;
use clap::CommandFactory;
use clap::FromArgMatches;
use std::alloc;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;
use tedge::command::BuildCommand;
use tedge::log::MaybeFancy;
use tedge::Component;
use tedge::ComponentOpt;
use tedge::TEdgeCli;
use tedge::TEdgeOpt;
use tedge::TEdgeOptMulticall;
use tedge_config::cli::CommonArgs;
use tedge_config::log_init_with_default_level;
use tedge_config::unconfigured_logger;
use tedge_file_log_plugin::bin::TEdgeConfigView;
use tracing::log;

// Control when to use console colors (`stdout` and `stderr` is a TTY, `NO_COLOR` is not set)
static USE_COLOR: yansi::Condition = yansi::Condition::from(|| {
    yansi::Condition::stdouterr_are_tty() && yansi::Condition::no_color()
});

#[global_allocator]
static ALLOCATOR: Cap<alloc::System> = Cap::new(alloc::System, usize::MAX);

// Register the Windows service dispatcher entry point.
//
// MSIX packaged services enforce that RegisterServiceCtrlHandlerExW is called
// from within a ServiceMain function (i.e. after StartServiceCtrlDispatcher
// has been called).  Calling it directly from main() returns
// ERROR_FAILED_SERVICE_CONTROLLER_CONNECT for packaged services even though
// the process was started by the SCM.  The define_windows_service! macro
// generates an extern "system" fn (ffi_service_main) that the dispatcher
// calls on a new thread; service_main_win is our Rust handler.
#[cfg(windows)]
windows_service::define_windows_service!(ffi_service_main, service_main_win);

/// ServiceMain callback invoked by StartServiceCtrlDispatcher on a dispatcher
/// thread.  RegisterServiceCtrlHandlerExW is called from here so it has the
/// SCM connection context it requires.
#[cfg(windows)]
fn service_main_win(_args: Vec<OsString>) {
    // Parse the process command line — same as main() does.
    let exec_name = executable_name();
    let opt = tracing::subscriber::with_default(unconfigured_logger(), || {
        parse_multicall(&exec_name, std::env::args_os())
    })
    .unwrap_or_else(|code| std::process::exit(code));

    // Create data dirs and seed default config before doing anything else.
    // This also creates the log directory used by register_with_scm's
    // diagnostic file.
    tedge::cli::windows_init::ensure_windows_data_dirs(&tedge_config::get_config_dir());

    // Register with the SCM.  Called from inside ServiceMain so
    // RegisterServiceCtrlHandlerExW has the SCM connection the OS requires.
    match &opt {
        TEdgeOptMulticall::Component(Component::TedgeMapper(m)) => {
            tedge::cli::windows_service_control::register_with_scm(&m.service_name());
        }
        TEdgeOptMulticall::Component(Component::TedgeAgent(_)) => {
            tedge::cli::windows_service_control::register_with_scm("tedge-agent");
        }
        _ => {}
    }

    // Run the service on a dedicated tokio runtime.  The outer runtime (from
    // #[tokio::main]) is blocked waiting for StartServiceCtrlDispatcher to
    // return, so we create a new one here for the actual service work.
    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(async {
            match opt {
                TEdgeOptMulticall::Component(Component::TedgeMapper(opt)) => {
                    let config =
                        tedge_config::TEdgeConfig::load(&opt.common.config_dir).await?;
                    log_memory_usage(config.run.log_memory_interval.duration());
                    tedge_mapper::run(opt, config).await
                }
                TEdgeOptMulticall::Component(Component::TedgeAgent(opt)) => {
                    let config =
                        tedge_config::TEdgeConfig::load(&opt.common.config_dir).await?;
                    log_memory_usage(config.run.log_memory_interval.duration());
                    tedge_agent::run(opt, config).await
                }
                _ => Ok(()),
            }
        });

    if let Err(e) = result {
        eprintln!("service error: {e:#}");
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = tracing::subscriber::with_default(unconfigured_logger(), || {
        clap_complete::CompleteEnv::with_factory(TEdgeCli::command).complete();

        parse_multicall(&executable_name(), std::env::args_os())
    })
    .unwrap_or_else(|code| std::process::exit(code));

    yansi::whenever(USE_COLOR);

    // On Windows, for service components (mapper / agent) use the service
    // control dispatcher so that RegisterServiceCtrlHandlerExW is called from
    // within the proper ServiceMain context.  MSIX packaged services require
    // this — calling the API directly from main() fails with
    // ERROR_FAILED_SERVICE_CONTROLLER_CONNECT.
    //
    // block_in_place lets tokio know the current thread will block so that it
    // can keep its worker pool healthy while StartServiceCtrlDispatcher runs.
    //
    // If the dispatcher itself fails (ERROR_FAILED_SERVICE_CONTROLLER_CONNECT)
    // the process is not running as a Windows service (e.g. interactive
    // terminal or `cargo run`), so we fall through to the normal async path.
    #[cfg(windows)]
    {
        let svc_name = match &opt {
            TEdgeOptMulticall::Component(Component::TedgeAgent(_)) => {
                Some("tedge-agent".to_string())
            }
            TEdgeOptMulticall::Component(Component::TedgeMapper(opt)) => {
                Some(opt.service_name())
            }
            _ => None,
        };
        if let Some(name) = svc_name {
            let result = tokio::task::block_in_place(|| {
                windows_service::service_dispatcher::start(&name, ffi_service_main)
            });
            match result {
                Ok(()) => return Ok(()),
                // Not a service context — fall through to the interactive path.
                Err(_) => {}
            }
        }
    }

    // Interactive / non-service path: run everything on the tokio runtime that
    // #[tokio::main] already created.

    // On Windows, bootstrap C:\ProgramData\tedge\ and default config files
    // before any service starts.  Safe to call repeatedly — all ops are
    // idempotent.
    #[cfg(windows)]
    if let TEdgeOptMulticall::Component(_) = &opt {
        tedge::cli::windows_init::ensure_windows_data_dirs(
            &tedge_config::get_config_dir(),
        );
    }

    match opt {
        TEdgeOptMulticall::Component(Component::TedgeMapper(opt)) => {
            let tedge_config = tedge_config::TEdgeConfig::load(&opt.common.config_dir).await?;
            log_memory_usage(tedge_config.run.log_memory_interval.duration());
            tedge_mapper::run(opt, tedge_config).await
        }
        TEdgeOptMulticall::Component(Component::TedgeAgent(opt)) => {
            let tedge_config = tedge_config::TEdgeConfig::load(&opt.common.config_dir).await?;
            log_memory_usage(tedge_config.run.log_memory_interval.duration());
            tedge_agent::run(opt, tedge_config).await
        }
        TEdgeOptMulticall::Component(Component::C8yFirmwarePlugin(fp_opt)) => {
            c8y_firmware_plugin::run(fp_opt).await
        }
        TEdgeOptMulticall::Component(Component::C8yRemoteAccessPlugin(opt)) => {
            let _ = c8y_remote_access_plugin::run(opt).await;
            Ok(())
        }
        TEdgeOptMulticall::Component(Component::TedgeWatchdog(opt)) => {
            tedge_watchdog::run(opt).await
        }
        TEdgeOptMulticall::Component(Component::TedgeWrite(opt)) => {
            tokio::task::spawn_blocking(move || tedge_write::bin::run(opt))
                .await
                .context("failed to run tedge write process")?
        }
        TEdgeOptMulticall::Component(Component::TedgeAptPlugin(opt)) => {
            let config = tedge_apt_plugin::get_config(opt.common.config_dir.as_std_path()).await;
            tokio::task::spawn_blocking(move || tedge_apt_plugin::run_and_exit(opt, config))
                .await
                .context("failed to run tedge apt plugin")?
        }
        TEdgeOptMulticall::Component(Component::TedgeFlowsPlugin(opt)) => {
            let config = tedge_flows_plugin::get_config(opt.common.config_dir.as_std_path()).await;
            tokio::task::spawn_blocking(move || tedge_flows_plugin::run_and_exit(opt, config))
                .await
                .context("failed to run tedge flows plugin")?
        }
        TEdgeOptMulticall::Component(Component::TedgeFileConfigPlugin(opt)) => {
            let tedge_config = tedge_config::TEdgeConfig::load(&opt.common.config_dir).await?;
            let tedge_config =
                tedge_file_config_plugin::bin::TEdgeConfigView::new(tedge_config.sudo.enable);
            tedge_file_config_plugin::bin::run(opt, tedge_config)
                .await
                .context("failed to run tedge file config plugin")
        }
        TEdgeOptMulticall::Component(Component::TedgeFileLogPlugin(opt)) => {
            let tedge_config = tedge_config::TEdgeConfig::load(&opt.common.config_dir).await?;
            let plugin_config = TEdgeConfigView::new(tedge_config.tmp.path.as_path());
            tokio::task::spawn_blocking(move || tedge_file_log_plugin::bin::run(opt, plugin_config))
                .await
                .context("failed to run tedge file log plugin")?
        }
        TEdgeOptMulticall::Tedge(TEdgeCli { cmd, common }) => {
            // Skip log initialisation for `tedge completions` — the command is
            // sourced from shell startup files and any warnings (e.g. about
            // unrecognised config keys) would be printed on every new shell session.
            if !matches!(cmd, TEdgeOpt::Completions { .. }) {
                log_init_with_default_level(
                    "tedge",
                    &common.log_args,
                    &common.config_dir,
                    tracing::Level::WARN,
                )?;
            }

            let tedge_config = tedge_config::TEdgeConfig::load(&common.config_dir).await?;

            let cmd = cmd
                .build_command(&tedge_config)
                .await
                .with_context(|| "missing configuration parameter")?;

            match cmd.execute(tedge_config).await {
                Ok(()) => Ok(()),
                // If the command already prints its own nicely formatted errors
                // don't also print the error by returning it
                Err(MaybeFancy::Fancy(_)) => std::process::exit(1),
                Err(MaybeFancy::Unfancy(err)) => {
                    Err(err.context(format!("failed to {}", cmd.description())))
                }
            }
        }
    }
}

fn log_memory_usage(log_memory_interval: Duration) {
    if log_memory_interval.is_zero() {
        return;
    }
    tokio::spawn(async move {
        loop {
            log::info!(
                "Allocated memory: {} Bytes {log_memory_interval:?}",
                ALLOCATOR.allocated()
            );
            tokio::time::sleep(log_memory_interval).await;
        }
    });
}

fn executable_name() -> Option<String> {
    Some(
        PathBuf::from(std::env::args_os().next()?)
            .file_stem()?
            .to_str()?
            .to_owned(),
    )
}

fn parse_multicall<Arg, Args>(
    executable_name: &Option<String>,
    args: Args,
) -> Result<TEdgeOptMulticall, i32>
where
    Args: IntoIterator<Item = Arg>,
    Arg: Into<OsString> + Clone,
{
    let cmd = TEdgeOptMulticall::command();

    let is_known_subcommand = executable_name
        .as_deref()
        .is_some_and(|name| cmd.find_subcommand(name).is_some());
    let cmd = cmd.multicall(is_known_subcommand);

    match cmd
        .try_get_matches_from(args)
        .and_then(|matches| TEdgeOptMulticall::from_arg_matches(&matches))
    {
        Ok(TEdgeOptMulticall::Tedge(TEdgeCli { cmd, common })) => {
            Ok(redirect_if_multicall(cmd, common))
        }
        Ok(t) => Ok(t),
        Err(e) => {
            let _ = e.print();

            if e.exit_code() == 0 {
                // e.g. --help was passed
                Err(0)
            } else if matches!(executable_name.as_deref(), Some("apt" | "tedge-apt-plugin")) {
                // Adhere to the plugin specification, which requires exit code 1 for invalid commands
                Err(1)
            } else {
                // For other commands, return the exit code from clap
                Err(e.exit_code())
            }
        }
    }
}

// Transform `tedge mapper|agent|write` commands into multicall commands
//
// This method has to be kept in sync with TEdgeOpt::build_command
fn redirect_if_multicall(cmd: TEdgeOpt, common: CommonArgs) -> TEdgeOptMulticall {
    match cmd {
        TEdgeOpt::Run(ComponentOpt { component }) => TEdgeOptMulticall::Component(component),
        cmd => TEdgeOptMulticall::Tedge(TEdgeCli { cmd, common }),
    }
}

#[cfg(test)]
mod tests {
    use crate::parse_multicall;
    use crate::Component;
    use crate::TEdgeOptMulticall;
    use test_case::test_case;

    #[test]
    fn launching_a_mapper() {
        let exec = Some("tedge-mapper".to_string());
        let cmd = parse_multicall(&exec, ["tedge-mapper", "c8y"]).unwrap();
        assert!(matches!(
            cmd,
            TEdgeOptMulticall::Component(Component::TedgeMapper(_))
        ))
    }

    #[test]
    fn using_tedge_to_launch_a_mapper() {
        let exec = Some("tedge".to_string());
        let cmd = parse_multicall(&exec, ["tedge", "run", "tedge-mapper", "c8y"]).unwrap();
        assert!(matches!(
            cmd,
            TEdgeOptMulticall::Component(Component::TedgeMapper(_))
        ))
    }

    #[test_case("tedge-mapper c8y --config-dir /some/dir")]
    #[test_case("tedge-mapper --config-dir /some/dir c8y")]
    #[test_case("tedge run tedge-mapper c8y --config-dir /some/dir")]
    #[test_case("tedge run tedge-mapper --config-dir /some/dir c8y")]
    #[test_case("tedge --config-dir /some/dir run tedge-mapper c8y")]
    // clap fails to raise an error here and takes the inner value for all global args
    #[test_case("tedge --config-dir /oops run tedge-mapper c8y --config-dir /some/dir")]
    fn setting_config_dir(cmd_line: &'static str) {
        let args: Vec<&str> = cmd_line.split(' ').collect();
        let exec = Some(args.first().unwrap().to_string());
        let cmd = parse_multicall(&exec, args).unwrap();
        match cmd {
            TEdgeOptMulticall::Component(Component::TedgeMapper(mapper)) => {
                assert_eq!(mapper.common.config_dir, "/some/dir")
            }
            _ => panic!(),
        }
    }

    #[test_case("apt --help", 0)]
    #[test_case("apt", 1)]
    #[test_case("apt list excessive arguments", 1)]
    #[test_case("tedge-apt-plugin --help", 0)]
    #[test_case("tedge-apt-plugin unknownarg", 1)]
    #[test_case("tedge-file-log-plugin --help", 0)]
    #[test_case("tedge-file-log-plugin unknownarg", 2)]
    #[test_case("tedge unknown", 2)]
    #[test_case("tedge --help", 0)]
    #[test_case("tedge", 2)]
    fn subcommands_exit_with_expected_codes(cmd_line: &'static str, expected_exit_code: i32) {
        let args: Vec<&str> = cmd_line.split(' ').collect();
        let exec = Some(args.first().unwrap().to_string());
        let res = parse_multicall(&exec, args);
        assert!(res.is_err());
        assert_eq!(res.err().unwrap(), expected_exit_code);
    }
}
