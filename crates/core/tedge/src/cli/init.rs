use crate::command::BuildContext;
use crate::command::Command;
use crate::Component;
use anyhow::bail;
use anyhow::Context;
use clap::Subcommand;
#[cfg(unix)]
use std::os::unix::fs;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[cfg(unix)]
use std::os::unix::fs::symlink as symlink;
#[cfg(windows)]
use std::os::windows::fs::symlink_file as symlink;

use std::path::Path;
use tedge_utils::file::change_user_and_group;
use tedge_utils::file::create_directory;
use tedge_utils::file::PermissionEntry;

#[derive(Debug)]
pub struct TEdgeInitCmd {
    user: String,
    group: String,
    relative_links: bool,
    context: BuildContext,
}

impl TEdgeInitCmd {
    pub fn new(user: String, group: String, relative_links: bool, context: BuildContext) -> Self {
        Self {
            user,
            group,
            relative_links,
            context,
        }
    }

    fn initialize_tedge(&self) -> anyhow::Result<()> {
        let executable_name =
            std::env::current_exe().context("retrieving the current executable name")?;
        #[cfg(unix)]
        let stat = std::fs::metadata(&executable_name).with_context(|| {
            format!(
                "reading metadata for the current executable ({})",
                executable_name.display()
            )
        })?;
        let Some(executable_dir) = executable_name.parent() else {
            bail!(
                "current executable ({}) does not have a parent directory",
                executable_name.display()
            )
        };
        let Some(executable_file_name) = executable_name.file_name() else {
            bail!(
                "current executable ({}) does not have a file name",
                executable_name.display()
            )
        };

        let component_subcommands: Vec<String> =
            Component::augment_subcommands(clap::Command::new("tedge"))
                .get_subcommands()
                .map(|c| c.get_name().to_owned())
                .chain(["tedge-apt-plugin".to_owned()])
                .collect();

        for component in &component_subcommands {
            let link = executable_dir.join(component);
            match std::fs::symlink_metadata(&link) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => bail!(
                    "couldn't read metadata for {}. do you need to run with sudo?",
                    link.display()
                ),
                meta => {
                    let file_exists = meta.is_ok();
                    if file_exists {
                        std::fs::remove_file(&link).with_context(|| {
                            format!("removing old version of {component} at {}", link.display())
                        })?;
                    }

                    let tedge = if self.relative_links {
                        Path::new(executable_file_name)
                    } else {
                        &*executable_name
                    };
                    symlink(tedge, &link).with_context(|| {
                        format!("creating symlink for {component} to {}", tedge.display())
                    })?;

                    #[cfg(unix)]
                    fs::lchown(&link, Some(stat.uid()), Some(stat.gid())).expect(format!("failed to change ownership of symlink at {}", link.display()).as_str());
                }
            }
        }

        let config_dir = self.context.config_location.tedge_config_root_path.clone();
        create_directory(
            &config_dir,
            PermissionEntry::new(
                Some(self.user.clone()),
                Some(self.group.clone()),
                Some(0o775),
            ),
        )?;

        create_directory(
            config_dir.join("mosquitto-conf"),
            PermissionEntry::new(
                Some(self.user.clone()),
                Some(self.group.clone()),
                Some(0o775),
            ),
        )?;
        create_directory(
            config_dir.join("operations"),
            PermissionEntry::new(
                Some(self.user.clone()),
                Some(self.group.clone()),
                Some(0o775),
            ),
        )?;
        create_directory(
            config_dir.join("operations").join("c8y"),
            PermissionEntry::new(
                Some(self.user.clone()),
                Some(self.group.clone()),
                Some(0o755),
            ),
        )?;
        create_directory(
            config_dir.join("plugins"),
            PermissionEntry::new(
                Some(self.user.clone()),
                Some(self.group.clone()),
                Some(0o775),
            ),
        )?;
        create_directory(
            config_dir.join("sm-plugins"),
            PermissionEntry::new(Some(self.user.clone()), Some(self.group.clone()), Some(0o755)),
        )?;
        create_directory(
            config_dir.join("device-certs"),
            PermissionEntry::new(Some(self.user.clone()), Some(self.group.clone()), Some(0o775)),
        )?;

        let config = self.context.config_repository.load()?;

        create_directory(
            config.logs.path.clone(),
            PermissionEntry::new(
                Some(self.user.clone()),
                Some(self.group.clone()),
                Some(0o775),
            ),
        )?;

        create_directory(
            &config.data.path,
            PermissionEntry::new(
                Some(self.user.clone()),
                Some(self.group.clone()),
                Some(0o775),
            ),
        )?;

        create_directory(
            config_dir.join(".tedge-mapper-c8y"),
            PermissionEntry::new(
                Some(self.user.clone()),
                Some(self.group.clone()),
                Some(0o775),
            ),
        )?;

        let entity_store_file = config_dir
            .join(".tedge-mapper-c8y")
            .join("entity_store.jsonl");

        if entity_store_file.exists() {
            change_user_and_group(entity_store_file.as_std_path(), &self.user, &self.group)?;
        }

        Ok(())
    }
}

impl Command for TEdgeInitCmd {
    fn description(&self) -> String {
        "Initialize tedge".into()
    }

    fn execute(&self) -> anyhow::Result<()> {
        self.initialize_tedge()
            .with_context(|| "Failed to initialize tedge. You have to run tedge with sudo.")
    }
}
