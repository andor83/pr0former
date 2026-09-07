# TouchDesigner via OSC or network MIDI

In a project's Score & parts view, set a part's OSC destination to the receiving machine's numeric IP and UDP port, for example `192.168.1.10:9000`. The default OSC address is `/pr0former/note` and arguments are integer MIDI pitch and velocity. Velocity zero represents note-off.

In TouchDesigner, configure an OSC In DAT on the matching UDP port. Use the incoming pitch/velocity values to drive your own visual parameters. Start the project transport; conducted/freeform projects can also launch individual parts. Network delivery is best effort and currently occurs at event time, without OSC timetags or receiver-side lookahead scheduling.

For MIDI, configure a macOS Network MIDI session in Audio MIDI Setup and connect it to the visual machine. Open pr0former Audio setup to find the exact exposed MIDI output name, then enter that name in the part's MIDI port field. Notes are sent on MIDI channel 1; stop/pause sends all-notes-off on all channels. A part can use both OSC and MIDI outputs simultaneously.

This example does not claim a tested TouchDesigner session. Automated tests exercise internal scheduling; actual network/device integration requires the receiving application and hardware.

In Score & parts, set OSC IP:port and OSC address before activation. Addresses are literal paths, for example `/touchdesigner/note`; wildcard patterns are not accepted as destinations. Each part can also select a MIDI port and channel 1–16. OSC messages carry integer pitch and velocity; velocity zero is note-off. MIDI and OSC may be enabled together.
