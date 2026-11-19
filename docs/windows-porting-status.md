# Windows porting status

The status of the Windows port is tracked by the OpenSpec change
[`openspec/changes/windows-port`](../openspec/changes/windows-port/proposal.md):

- the requirements are in its `specs/` directory,
- the design decisions (service stop, process trees, plugin rules, child-device directories, ACLs, system tests) are in `design.md`,
- the remaining work is in `tasks.md`.

The user documentation, including the Windows support matrix, is
[docs/src/install/windows.md](src/install/windows.md).

Packaging and the winget plugin are tracked by the
`windows-msi-packaging` and `windows-sm-plugin` changes.
