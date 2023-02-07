# Demo client

## Usage

```
demo_client <HOST> <TOPIC> <FILE> <COLUMN> <INTERVAL>
```

example to publish a value from the second column of the provided sample file every second:
```sh
demo_client mqtts://localhost:1883 demo-test mannheim_avrg_january.csv 1 1
```

