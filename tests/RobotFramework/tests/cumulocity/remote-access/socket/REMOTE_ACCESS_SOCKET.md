


### 

Currently when c8y-remote-access-plugin is launched, it is launched as a child process of the `tedge-mapper-c8y` which means that if the mapper service is killed, then the c8y-remote-access-plugin will also be killed.

Below shows the process relationship between the mapper and the c8y-remote-access-plugin process.

```sh
# systemctl status tedge-mapper-c8y
● tedge-mapper-c8y.service - tedge-mapper-c8y converts Thin Edge JSON measurements to Cumulocity JSON format
     Loaded: loaded (/lib/systemd/system/tedge-mapper-c8y.service; enabled; vendor preset: enabled)
     Active: active (running) since Thu 2022-04-28 17:42:32 UTC; 2 years 0 months ago
    Process: 430 ExecStartPre=/usr/bin/tedge init (code=exited, status=0/SUCCESS)
   Main PID: 444 (tedge-mapper)
      Tasks: 14 (limit: 4163)
     CGroup: /system.slice/tedge-mapper-c8y.service
             ├─   444 /usr/bin/tedge-mapper c8y
             └─ 12537 /usr/bin/c8y-remote-access-plugin --child 530,rpi4-dca632486720,127.0.0.1,22,2b358a8a-e503-47bd-9785-457faca83bb6
```

One way of solving this would be to launch the c8y-remote-access service via a socket, where the socket will spawn a new instance of the c8y-remote-access-plugin.

Below shows a new socket service type: `c8y-remote-access-plugin.socket`

```sh
# systemctl status c8y-remote-access-plugin.socket
● c8y-remote-access-plugin.socket - c8y-remote-access-plugin Socket
     Loaded: loaded (/lib/systemd/system/c8y-remote-access-plugin.socket; enabled; preset: enabled)
     Active: active (listening) since Thu 2024-05-02 13:46:09 UTC; 9min ago
   Triggers: ● c8y-remote-access-plugin@0-127.0.0.1:4444-127.0.0.1:39576.service
     Listen: 127.0.0.1:4444 (Stream)
   Accepted: 1; Connected: 1;
      Tasks: 0 (limit: 11858)
     Memory: 8.0K
        CPU: 858us
     CGroup: /system.slice/c8y-remote-access-plugin.socket

May 02 13:46:09 2c18d9f4c2d1 systemd[1]: Listening on c8y-remote-access-plugin.socket - c8y-remote-access-plugin Socket.
```

On incoming socket connections, a new service will be spawned from the give template service (c8y-remote-access-plugin@.service), which results in a service which will only exist for as long as the c8y-remote-access-plugin process exists. Below is an example of such a service instance (notice the long service name as it shows the local socket connection)

```sh
# systemctl status c8y-remote-access-plugin@0-127.0.0.1:4444-127.0.0.1:39576.service
● c8y-remote-access-plugin@0-127.0.0.1:4444-127.0.0.1:39576.service - c8y-remote-access-plugin Service (127.0.0.1:39576)
     Loaded: loaded (/lib/systemd/system/c8y-remote-access-plugin@.service; disabled; preset: enabled)
     Active: active (running) since Thu 2024-05-02 13:46:42 UTC; 9min ago
TriggeredBy: ● c8y-remote-access-plugin.socket
   Main PID: 695 (c8y-remote-acce)
      Tasks: 5 (limit: 11858)
     Memory: 564.0K
        CPU: 12ms
     CGroup: /system.slice/system-c8y\x2dremote\x2daccess\x2dplugin.slice/c8y-remote-access-plugin@0-127.0.0.1:4444-127.0.0.1:39576.service
             └─695 /usr/bin/c8y-remote-access-plugin --child -

May 02 13:46:42 2c18d9f4c2d1 systemd[1]: Started c8y-remote-access-plugin@0-127.0.0.1:4444-127.0.0.1:39576.service - c8y-remote-access-plugin Service (127.0.0.1:39576).
```

The above service is independent of the tedge-mapper-c8y (ignoring that fact that c8y-remote-access-plugin uses the `localhost:8001/c8y` proxy which is also provided by the tedge-mapper-c8y service):

```sh
# systemctl status tedge-mapper-c8y
● tedge-mapper-c8y.service - tedge-mapper-c8y converts Thin Edge JSON measurements to Cumulocity JSON format.
     Loaded: loaded (/lib/systemd/system/tedge-mapper-c8y.service; enabled; preset: enabled)
     Active: active (running) since Thu 2024-05-02 13:46:10 UTC; 1min 22s ago
    Process: 628 ExecStartPre=/usr/bin/tedge init (code=exited, status=0/SUCCESS)
   Main PID: 636 (tedge-mapper)
      Tasks: 7 (limit: 11858)
     Memory: 1.5M
        CPU: 374ms
     CGroup: /system.slice/tedge-mapper-c8y.service
             └─636 /usr/bin/tedge-mapper c8y
```



### Show active remote-access sessions

```sh
# systemctl status
c8y-remote-access-plugin@39-127.0.0.1:4444-127.0.0.1:51760.service loaded active     running   c8y-remote-access-plugin Service (127.0.0.1:51760)
```


### Check socket status

```sh
# systemctl status c8y-remote-access-plugin.socket
● c8y-remote-access-plugin.socket - c8y-remote-access-plugin Socket
     Loaded: loaded (/lib/systemd/system/c8y-remote-access-plugin.socket; disabled; preset: enabled)
     Active: active (listening) since Wed 2024-05-01 19:56:31 UTC; 1h 34min ago
   Triggers: ● c8y-remote-access-plugin@39-127.0.0.1:4444-127.0.0.1:51760.service
     Listen: 127.0.0.1:4444 (Stream)
   Accepted: 40; Connected: 1;
      Tasks: 0 (limit: 11858)
     Memory: 8.0K
        CPU: 18ms
     CGroup: /system.slice/c8y-remote-access-plugin.socket

May 01 19:56:31 8aadb3801860 systemd[1]: Listening on c8y-remote-access-plugin.socket - c8y-remote-access-plugin Socket.
```
