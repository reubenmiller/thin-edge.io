## MODIFIED Requirements

### Requirement: Graceful shutdown for standalone processes

Standalone processes SHALL shut down gracefully on SIGINT, SIGTERM, or SIGQUIT,
using the supervisor's shutdown mechanism.
On Windows, they SHALL shut down gracefully on the console Ctrl+C, Ctrl+Break, close and shutdown events,
and on a Service Control Manager stop request.

#### Scenario: SIGTERM shuts down a standalone process

- **WHEN** a standalone process receives SIGTERM, SIGINT, or SIGQUIT
- **THEN** the supervisor drains the component within the shutdown timeout and exits

#### Scenario: Windows Service stop shuts down a standalone process

- **WHEN** the SCM sends a stop request to a standalone process running as a Windows Service
- **THEN** the supervisor drains the component within the shutdown timeout and exits

#### Scenario: Second termination signal forces exit

- **WHEN** a standalone process receives a second termination signal during shutdown
- **THEN** the supervisor aborts and exits immediately
