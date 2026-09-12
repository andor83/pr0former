# Bundled sample credits

The server ships one small drum kit. On every start it seeds the six
recordings into the shared sample library as **global samples** ("Kick
(bundled kit)" and so on, author `pr0former`, root note 60, category Drums,
tags `drum`, `kit`, `cc0`), so
every account sees them in the sample browser and can add them to any project
without uploading anything. Inserting the **Drum Sampler** library subgraph
links a project to those global samples so the kit plays immediately; if an
administrator has deleted a bundled sample, the subgraph falls back to a
private copy of the embedded bytes. Seeding is idempotent and never
resurrects a sample an administrator removed. The files live in
`crates/server/assets/drums` and are compiled into the server binary.

All six recordings are dedicated to the public domain under the
[CC0 1.0 Universal](https://creativecommons.org/publicdomain/zero/1.0/)
dedication. They were recorded by **menegass** and published on
[freesound.org](https://freesound.org); they reached this project through the
[Sonic Pi](https://github.com/sonic-pi-net/sonic-pi) sample set
(`etc/samples`), which states that every bundled sample is CC0. Attribution is
not required by CC0; it is kept here as a courtesy and so the sources can be
traced.

| Pad | Bundled file | Sonic Pi name | Source |
| --- | --- | --- | --- |
| Bass drum | `kick.wav` | `drum_bass_hard` | https://www.freesound.org/people/menegass/sounds/100051/ |
| Snare | `snare.wav` | `drum_snare_hard` | https://www.freesound.org/people/menegass/sounds/100058/ |
| Tom 1 | `tom-1.wav` | `drum_tom_lo_hard` | https://www.freesound.org/people/menegass/sounds/100064/ |
| Tom 2 | `tom-2.wav` | `drum_tom_hi_hard` | https://www.freesound.org/people/menegass/sounds/100062/ |
| Hi hat | `hi-hat.wav` | `drum_cymbal_closed` | https://www.freesound.org/people/menegass/sounds/100053/ |
| Cymbal | `cymbal.wav` | `drum_cymbal_hard` | https://www.freesound.org/people/menegass/sounds/100056/ |

Conversion: the original mono 44.1 kHz FLAC files were converted with FFmpeg
to 48 kHz stereo 16-bit PCM WAV with metadata stripped, so they match the
stereo voices of the preset and the project sample cache. Total size is under
one megabyte.

## Other kits considered

- [Boochi44/free-drum-samples](https://github.com/Boochi44/free-drum-samples):
  three CC0 electronic kits (trap, bounce, vintage), partly derived from the
  CC0 TR-808 set by Edward Loveall. A good candidate for an electronic kit.
- [Versilian Community Sample Library](https://github.com/sgossner/VCSL):
  large CC0 orchestral and percussion library; too big to bundle wholesale.
- [AVL Drumkits](https://www.bandshed.net/avldrums/) (Black Pearl, Red
  Zeppelin): excellent multi-velocity acoustic kits, but CC BY-SA 3.0, whose
  share-alike terms do not fit this MIT-licensed repository.
- Sites such as freewavesamples.com, Cymatics and MusicRadar are free to
  download but forbid redistribution, so they cannot be bundled.
