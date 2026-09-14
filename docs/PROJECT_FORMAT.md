# Portable projects (`.pr0`)

Project browser → Export downloads a standard ZIP with a `.pr0` extension:

```
project.json
samples/<source-asset-id>.wav
```

`project.json` contains `format: "pr0former-project"`, archive `version: 1`,
`project_schema: 1`, the project, and `software` metadata:

- `name`: `pr0former`
- `version`: the exporting server's Cargo package version
- `build.git_commit`, `build.dirty`, and `build.built_at_unix`

Software versions and schema versions are separate. The importer rejects unknown
archive/schema versions and exports from newer software with an update message.
Older software versions with the supported schema are accepted only after current
server-side project validation. There is no general-purpose migration framework
yet. The build identity distinguishes development builds sharing a package version;
it is recorded for diagnostics, not used to order Git commits.

All project-local original sample WAVs are included, even unused samples.
Generated resampling caches are regenerated. Import allocates fresh project-local
asset IDs and rewrites sample-node assets and sample-shortlist references. External
controllers/scripts which hard-code numeric sample IDs need their IDs updated.
Machine-local and user assignments are cleared. Node settings and score data persist.
Sample catalog names/tags and recorder-track files are not included in this version.

On a naming conflict, the browser offers Rename & import, Cancel, and Overwrite
existing. Overwrite uses the target's revision and permissions, retains its identity,
and does not replace existing WAV files referenced by old revisions. It can repair
an earlier broken JSON import without first opening that target's missing audio.

Unversioned raw JSON and the older JSON wrapper remain importable. They do not
contain audio; missing referenced WAVs are rejected with instructions to export a
`.pr0` file from the source project. Do not delete the source to repair a JSON import.

The upload and total expanded content limits are 256 MiB, with a 4 MiB project
manifest and at most 1,025 entries. Imported WAVs must have 1–8 channels and at most
30 seconds. Unexpected archive paths, duplicate entries, malformed WAVs, nonfinite
float samples, unsupported versions, and missing referenced audio are rejected.
Archive names are parsed as numeric IDs and never used as extraction paths.
