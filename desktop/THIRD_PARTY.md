pr0former desktop contains FFmpeg (https://ffmpeg.org), built without GPL or
nonfree components under LGPL 2.1. The exact FFmpeg source archive, LGPL text
and build script are included in resources/licenses/ffmpeg. No FFmpeg source
modifications are applied. This executable is invoked as a separate process.

The application and server also use dependencies under their respective open
source licenses; bundled dependency texts and an inventory are in the dependencies
folder. See also the two Cargo.lock files and web/package-lock.json. Release
publishers must retain dependency notices, audit any added native libraries and
provide corresponding FFmpeg source with their downloads. An arbitrary system
FFmpeg build is not substituted by build.sh.
