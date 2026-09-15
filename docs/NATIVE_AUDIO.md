# Native device routing

In **System settings → System audio**, enable the desired interface. In an
**Audio output** or **Audio input** node's options, select that interface and
map signal channels to its hardware channels. The engine currently uses f32
streams at the selected global sample rate. Unsupported configurations report
an error; they do not silently fall back to another interface.

## Linux

- Named PulseAudio sinks/sources are discovered with `pactl --format=json`.
  PipeWire's PulseAudio compatibility server exposes the same interface.
  Each route opens its own `pulse:DEVICE=<endpoint-name>` ALSA PCM: selection
  does not change the global default or move other applications' streams.
  Endpoint channel counts constrain stream configuration instead of using the
  Pulse ALSA plugin's advertised maximum. The ALSA Pulse plugin must be installed
  (often `libasound2-plugins`); discovery needs `pactl` (often `pulseaudio-utils`
  or `libpulse`). Package names vary by distribution.
- Direct ALSA routes enumerate `/proc/asound/pcm`, including nonzero HDMI PCM
  numbers, and open `hw:CARD=<stable-card-id>,DEV=<pcm-number>`. These bypass
  desktop-server mixing and can require exclusive access. They do not add an
  implicit resampler or format converter. A device lacking a compatible f32
  configuration cannot currently use the direct path; it may work through the
  PulseAudio/PipeWire route instead. Integer-format native stream support is
  not implemented by this change.
- Explicit Linux routes start **disabled** to avoid duplicate playback through
  `default`, `pulse`, `pipewire`, and the underlying hardware simultaneously.
  Disable unused generic routes before enabling the desired explicit route.
- The Linux details panel shows sink/source descriptions, defaults, channel maps,
  active ports, port availability, and card profiles. An inactive HDMI/analog
  profile is information, not an independently opened output. Change profiles
  and ports in the OS sound settings first. `pw-dump` is a diagnostic fallback
  when Pulse discovery is unavailable; this is not a native PipeWire streaming
  implementation. Pulse monitor sources are not added as physical inputs.
- Run the performance server as the desktop-session user. A system service or
  another account may see a different sound server or none. Discovery never
  starts services, changes defaults, or writes ALSA configuration.

The vendored CPAL patch skips OSS discovery when `/dev/dsp` is absent, rather
than suppressing ALSA errors globally. Native inventory metadata is single-flight
cached for 30 seconds per sample rate; Linux details cache for five seconds.
Reopen settings after that interval following a device/profile change. Discovery
releases device handles and runs off the audio worker. Stream startup still
validates the selected route. Tools have three-second timeouts and 2 MiB output
limits. Missing tools/session access produce an actionable diagnostic.

## Windows

WASAPI exposes individual playback/capture endpoints, including HDMI, USB and
analog endpoints available through the installed driver. Friendly labels are
displayed; opaque `IMMDevice::GetId` identities are saved as route names and
hashed to the graph's numeric interface ID. Identically named endpoints remain
distinct. Existing unambiguous name-based selections upgrade without changing
their numeric graph route IDs or enabled/latency settings. Ambiguous legacy
names fail rather than selecting the first matching device; reselect them in
System settings. Endpoint identities can change after device/driver replacement.

This is the existing WASAPI **shared-mode** backend. It does not add ASIO,
exclusive-mode WASAPI, clock synchronization across interfaces, or a verified
low-latency pro-audio contract. Channel availability follows the driver's
exposed endpoint configuration. Future ASIO/native PipeWire work remains separate.

macOS continues using the existing CoreAudio selection and channel mapping.

## Validation boundary

Parser/routing-identity tests and browser fixtures verify software contracts,
not physical playback. The modified CPAL Windows backend passes a Windows-target
Rust check. Native Linux/Windows playback, hotplug, driver restart, multichannel
USB/HDMI, and latency remain manual until exercised on those systems.

Upstream references: [ALSA Pulse PCM device argument](https://github.com/alsa-project/alsa-plugins/blob/master/pulse/50-pulseaudio.conf),
[PulseAudio inventory fields](https://github.com/pulseaudio/pulseaudio/blob/master/src/utils/pactl.c),
[PipeWire pw-dump](https://docs.pipewire.org/page_man_pw-dump_1.html), and
[Windows endpoint identities](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdevice-getid).
