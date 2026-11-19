# Software management plugin used by the Windows system tests.
# It lists a single package and accepts every other command.
# The software type is the file name without the extension: winlist
$command = $args[0]
switch ($command) {
    "list" {
        Write-Output "robot-windows-package`t1.2.3"
        exit 0
    }
    "update-list" {
        # Not supported: the agent falls back to install/remove
        exit 1
    }
    default {
        # prepare, install, remove and finalize
        exit 0
    }
}
