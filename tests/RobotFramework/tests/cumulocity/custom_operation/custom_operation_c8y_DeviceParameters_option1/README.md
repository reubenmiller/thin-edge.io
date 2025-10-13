
### Create a property identifier

```sh
c8y api POST /service/dtm/definitions/properties --template '{
  "identifier": "AutoUpdater",
  "jsonSchema": {
    "$schema": "http://json-schema.org/draft-07/schema#",
    "title": "Auto Updater",
    "description": "Auto update configuration to keep your device up to date with the latest software",
    "type": "object",
    "properties": {
      "enabled": {
        "type": "boolean"
      },
      "interval": {
        "type": "string"
      }
    }
  },
  "tags": [
    "thin-edge.io"
  ],
  "contexts": ["event", "asset"],
  "additionalProp1": {}
}
'
```

### Delete a property identifier

```sh
c8y api DELETE "/service/dtm/definitions/properties/AutoUpdater?contexts=asset,event"
```

### Update a property identifier

TODO the following command fails. Maybe not all fields can be updated.

```sh
c8y api PUT "/service/dtm/definitions/properties/AutoUpdater?contexts=event,asset" --template '{
  "identifier": "AutoUpdater",
  "jsonSchema": {
    "$schema": "http://json-schema.org/draft-07/schema#",
    "title": "Auto Updater",
    "description": "Auto update configuration to keep your device up to date with the latest software",
    "type": "object",
    "properties": {
      "enabled": {
        "type": "boolean"
      },
      "interval": {
        "type": "string",
        "enum": ["hourly", "daily", "weekly"]
      }
    }
  },
  "tags": [
    "thin-edge.io"
  ],
  "contexts": ["event", "asset"],
  "additionalProp1": {}
}
'
```

## SmartREST 2.0 Templates

* https://cumulocity.com/docs/smartrest/mqtt-static-templates/#532

* https://cumulocity.com/docs/smartrest/mqtt-static-templates/#408
