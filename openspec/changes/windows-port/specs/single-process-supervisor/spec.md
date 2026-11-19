## MODIFIED Requirements

### Requirement: Graceful shutdown on termination signals

The supervisor SHALL own process signal handling and perform an orderly shutdown of
all components on a termination signal.
On Windows, the termination signals are the console Ctrl+C, Ctrl+Break, close and shutdown events.
The supervisor SHALL also perform this shutdown when a shutdown is requested programmatically,
which is how a Windows Service Control Manager stop request is delivered.

#### Scenario: SIGTERM drains all components

- **WHEN** the process receives SIGTERM, SIGINT, or SIGQUIT
- **THEN** the supervisor requests each component to shut down, waits for them to drain within the shutdown timeout, and then exits

#### Scenario: Windows console event drains all components

- **WHEN** the process receives a Ctrl+C, Ctrl+Break, console close or shutdown event on Windows
- **THEN** the supervisor requests each component to shut down, waits for them to drain within the shutdown timeout, and then exits

#### Scenario: Programmatic shutdown request

- **WHEN** a shutdown is requested programmatically, before or after the supervisor started listening
- **THEN** the supervisor drains all components as for a termination signal

#### Scenario: Second termination signal forces exit

- **WHEN** a second termination signal arrives, or the shutdown timeout expires
- **THEN** the supervisor aborts remaining components and exits immediately

### Requirement: Restart mappers on SIGUSR1

The supervisor SHALL restart all mapper components, and only mapper components, when
it receives SIGUSR1.
On Windows, which has no SIGUSR1, mappers are restarted by restarting the service.

#### Scenario: SIGUSR1 restarts mappers only

- **WHEN** the process receives SIGUSR1
- **THEN** every mapper component is restarted
- **AND** the agent continues running uninterrupted

#### Scenario: Restart requests are coalesced

- **WHEN** a restart is requested for a component that is already restarting or in restart backoff
- **THEN** the request is ignored rather than queued or stacked
