# pr0former CPAL patch

Based on the crates.io CPAL 0.16.0 release (Apache-2.0; see LICENSE).
Sources, examples, build script and normalized manifest are retained locally.

`src/host/alsa/enumerate.rs`: skip the built-in OSS probe when `/dev/dsp`
does not exist. Upstream enumeration opens built-in PCM devices before yielding
them, so filtering returned names in our application cannot prevent the error.
No global ALSA error handler is installed: genuine device errors remain visible.
`src/host/alsa/mod.rs`: a lazy `from_pcm_name` constructor lets the application
open a validated, explicit `pulse:DEVICE=...` or `hw:CARD=...,DEV=...` route
without enumerating unrelated devices. It does not alter global ALSA config.

`src/host/wasapi/device.rs`: expose the opaque `IMMDevice::GetId` endpoint
identity, freeing its COM-allocated string. Friendly names remain display labels.
This avoids collapsing identically named endpoints into one route.

Other platform backends are unchanged. Reassess these patches on CPAL upgrades.
