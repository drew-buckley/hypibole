# hypibole

Hypibole is a dead simple, lightweight HTTP API for driving and reading whitelisted GPIOs on a Raspberry Pi. I've been primarily running this on a Raspberry Pi Zero W. Someday I will find the time to improve this doc, but for now, if you really want to understand all aspects about how to use this, you'll have to read the source. Currently, GPIOs can only be controlled as discrete digital I/O pins. 

## Examples

The following are some example uses of the API: 
`http://pi.local:8080/?pin=4&op=get` --> Reads the current state of pin 4.
`http://pi.local:8080/?pin=4&op=set&level=high` --> Drives pin 4 high.

The API call will respond with a JSON string describing what happened and if the operation was successful: 
`{"level":"low","operation":"get","pin":"4","status":"success"}` --> A `get` operation succeeded, and the pin's level is low.
`{"error":"Failed to perform board operation: \"Could not find pin 5 in either map.\""}` --> Operation failed because pin 5 was not whitelisted. 
