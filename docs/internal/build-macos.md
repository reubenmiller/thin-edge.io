
## Build

### MacOS aarch64

```
rustup target add aarch64-apple-darwin
cargo build --target=aarch64-apple-darwin
```

### MacOS x86_64

```
rustup target add x86_64-apple-darwin
cargo build --target=x86_64-apple-darwin
```

### Running on MacOS


```sh
mkdir -p "$HOME/tedge"
cat << EOT > "$HOME/tedge"
[logs]
path = "$HOME/tedge/logs"

[data]
path = "$HOME/tedge/data"

[http]
# Optional: Change port (if port 8000 is already used by another process)
bind.port=8010
client.port=8010
EOT
```

Start the process

```sh
cargo run --bin tedge -- init --relative-links --config-dir "$HOME/tedge" --user "$USER" --group staff
```

```sh
target/debug/tedge-agent --config-dir "$HOME/tedge"
```

### Using

#### Send operation

**Command: config_snapshot**

```sh
./target/debug/tedge mqtt pub 'te/device/main///cmd/config_snapshot/local-1234' '{"status":"init","type":"tedge-configuration-plugin","tedgeUrl":"http://localhost:8005/tedge/file-transfer/config/tedge-configuration-plugin"}' -r
```

**Command: software_list**

```sh
./target/debug/tedge mqtt pub 'te/device/main///cmd/software_list/local-1234' '{"status":"init"}' -r
```


## Improvements

### Command: Make tedgeUrl optional for config_snapshot

Why does the `config_snapshot` snapshot require the `tedgeUrl` property? This makes it harder to request data from thin-edge.io, and you need to know the file transfer information.

```sh
tedge mqtt pub 'te/device/main///cmd/config_snapshot/local-1234' '{"status":"init","type":"tedge-configuration-plugin","tedgeUrl":"http://localhost:8005/tedge/file-transfer/config/tedge-configuration-plugin"}' -r
```

### sudo.enable config is not respected everywhere

Expected: sm-plugin (and all components) should respect the `sudo.enable` setting!

* crates/core/tedge_agent/src/restart_manager/actor.rs
* crates/core/tedge_agent/src/software_manager/actor.rs


### Setting config-dir or the tedge.toml path using env would be useful


