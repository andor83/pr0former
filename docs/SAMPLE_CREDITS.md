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
to 48 kHz stereo 16-bit with metadata stripped, so they match the stereo voices
of the preset and the project sample cache.

## How bundled audio is stored

Bundled audio ships as **FLAC** and is expanded to WAV when a sample is first
seeded, by `sample_library::wav_bytes`. Symphonia already decodes FLAC for the
sample importer, so this needs no new dependency and still cross-compiles for
iOS, and FLAC is lossless, so the stored WAV is bit-identical to the WAV the
kit shipped as before. Seeding is idempotent and skips samples that already have
a library row, so a first install expands everything once and an update that adds
a sample expands only that one.

All eight bundled files total about 1.2 MB as FLAC, against 4.1 MB as WAV.

To add or replace one, convert to 48 kHz stereo 16-bit FLAC:

```sh
ffmpeg -y -i source.wav -ar 48000 -ac 2 -c:a flac -compression_level 12 \
  -sample_fmt s16 -map_metadata -1 -fflags +bitexact crates/server/assets/<dir>/<name>.flac
```

## Licence policy for bundled samples

**Only CC0, public-domain or equivalently permissive material may be bundled or
seeded.** A sample that requires attribution would pass that obligation on to
every performer who uses pr0former in their own work, which is not acceptable
for material the application ships by default. CC BY, CC BY-SA, CC BY-NC and the
legacy Freesound Sampling+ terms are therefore all out, however convenient the
sound. Courtesy credit is still recorded in this file for everything bundled, so
sources can be traced; the difference is that nobody downstream is obliged to
repeat it.

This rules out the CC BY and CC BY-NC sources listed further down as fallbacks:
they remain useful as things a user may download for their own project, but they
must not be added to `crates/server/assets` or the seeded library.

## Other bundled samples

Samples seeded into the shared library that are not drum pads live in
`crates/server/assets` alongside the kit and are listed in `BUNDLED_EXTRAS`.
They are seeded the same way — global, idempotent, never resurrected after an
administrator deletes them — but carry their own category and tags.

| Bundled file | Library name | Category | Source | License |
| --- | --- | --- | --- | --- |
| `voices/atari-speech.flac` | Atari speech (bundled) | Voices | https://freesound.org/people/Timbre/sounds/547419/ | [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/) |
| `instruments/music-box.flac` | Music box (bundled) | Instruments | https://freesound.org/people/Flying_Deer_Fx/sounds/369405/ | [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/) |

"2020 remix of i-have-an-atari-speech-synthesizer-and-i-m-not-afraid-to-use-it"
by **Timbre**: an Atari speech synthesizer recording put through a
small-speaker impulse response, 5.2 s. Dedicated to the public domain under
CC0 1.0, so attribution is not required; it is recorded here as a courtesy and
for traceability, as with the kit.

Conversion: Freesound serves the 44.1 kHz mono 16-bit FLAC original only to
signed-in users, so the bundled file was made from the public HQ MP3 preview
(`https://cdn.freesound.org/previews/547/547419_1015240-hq.mp3`) with FFmpeg,
to 48 kHz stereo 16-bit PCM WAV with metadata stripped. **It therefore carries
MP3 encoding artifacts the original does not.** To replace it with the
lossless original, download the FLAC from the sound page while signed in and
re-run:

```sh
ffmpeg -y -i 547419__timbre__*.flac -ar 48000 -ac 2 -c:a flac -sample_fmt s16 \
  -map_metadata -1 -fflags +bitexact crates/server/assets/voices/atari-speech.flac
```

The WAV is about 1 MB, which is roughly the size of the whole drum kit; the
stereo conversion doubles a mono source to match the project convention.

### Music box

"Music Box - J. Brahms - Opus 39 - Waltz no 3" by **Flying_Deer_Fx**: a music
box playing Brahms's waltz, 59 s. Dedicated to the public domain under CC0 1.0,
so attribution is not required; it is recorded here as a courtesy and for
traceability, as with the kit. The waltz itself is long out of copyright.

This replaced an earlier CC BY 4.0 music box, which was removed: see the licence
policy above.

Conversion: as with the Atari sound, the lossless original is behind a Freesound
login, so the bundled file was made from the public HQ MP3 preview
(`https://cdn.freesound.org/previews/369/369405_6812364-hq.mp3`) with FFmpeg, to
48 kHz stereo 16-bit PCM WAV with metadata stripped, and **carries MP3
artifacts**. To replace it with the original, download it while signed in and
re-run:

```sh
ffmpeg -y -i 369405__flying_deer_fx__*.wav -ar 48000 -ac 2 -c:a flac -sample_fmt s16 \
  -map_metadata -1 -fflags +bitexact crates/server/assets/instruments/music-box.flac
```

Only the first **10 seconds** are bundled, with a 0.6 s fade-out, because the
sample is example material rather than a work to be reproduced: the full 59 s
came to 11.4 MB of WAV, which dwarfed everything else shipped. The excerpt is
731 KB as FLAC. To bundle a different span, adjust `-ss`/`-t` in the command
above.

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

## Orchestral instrument samples (recommended source, not bundled)

pr0former does not ship orchestral samples; a usable set is several gigabytes.
For samplers that need sustained orchestral notes, the recommended source is
**VSCO 2 Community Edition** by Versilian Studios LLC (Sam Gossner, Simon
Dalzell), dedicated to the public domain under
[CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/):
https://github.com/sgossner/VSCO-2-CE (WAV, 44.1 kHz, pitch in each file name,
no account needed). It covers violin, viola, cello and bass sections, solo
violin and contrabass, harp, flute, piccolo, oboe, clarinet, bassoon, horn,
trumpet, trombone, tuba, timpani and mallet percussion, and is mirrored on
Freesound by user **Samulis** (packs named "VSCO 2 CE - …"). Attribution is not
required by CC0; a courtesy line is: "Orchestral samples from VSCO 2 Community
Edition by Versilian Studios LLC, CC0 1.0, https://github.com/sgossner/VSCO-2-CE".
The larger **VCSL** (https://github.com/sgossner/VCSL) is the same authors'
CC0 superset.

Fallbacks and their obligations:

| Source | License | Notes |
| --- | --- | --- |
| MTG "good-sounds" packs, https://freesound.org/people/MTG/packs/ | CC BY 3.0 | Dense chromatic single notes for violin, cello, double bass, flute, piccolo, oboe, clarinet, trumpet, saxes; no viola, bassoon, brass low end or percussion. Each file used must be credited: title, author "MTG (Music Technology Group, Universitat Pompeu Fabra)", its Freesound URL, the license link https://creativecommons.org/licenses/by/3.0/ and a note of any conversion. |
| pjcohen orchestral percussion, https://freesound.org/people/pjcohen/packs/ | CC0 or CC BY 4.0 per sound | Concert bass drum, hand cymbals, tam-tam, celesta, double bass. Check each sound; CC BY 4.0 files need the same credit shape with https://creativecommons.org/licenses/by/4.0/. |
| University of Iowa Musical Instrument Samples, https://theremin.music.uiowa.edu/MIS.html | Informal "no restrictions" statement | Complete instrument coverage, but many files hold several notes that must be split; not an SPDX license. |

Not redistributable here: the Philharmonia Orchestra samples (free to use but
may not be re-shipped as samples or a sampler instrument) and Freesound packs
under CC BY-NC or the legacy Sampling+ terms, such as Carlos_Vaquero's string
and wind packs.

Add every sample actually shipped or seeded to one of the tables above with its
source URL and license before it lands in `crates/server/assets` or the seeded
library.
