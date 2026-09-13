# Documentation and contextual help

The in-app guide is available from Settings → Help & documentation, the desktop
Help menu, and `/?help=1` (including before account sign-in). Setup instructions
live in `web/src/components/SetupGuide.vue`. They cover first sound, the server/client
model, desktop local/remote/LAN operation, `init.sh`, and `build.sh`. Keep script
flags and native menu labels aligned with their source implementations.

## Node authors

A node's reference belongs to its catalog descriptor in `pr0-core`, not to an
independent frontend example generator. `Descriptor.documentation` carries a
`NodeDocumentation` object from `crates/core/src/documentation.rs`:

- `title`: a concrete musical or programming task.
- `explanation`: what this patch accomplishes and why the connections/settings work.
- `steps`: any device selection, sample assignment, score setup, or operating steps.
- `graph`: ordinary `Graph` nodes and edges with real parameter IDs, channel widths,
  port contracts, and explicit example parameter overrides.

Built-in authors edit `crates/core/src/node_documentation.json`, keyed by the
catalog's node kind. The catalog attaches that entry to each descriptor; both the
node reference and documentation graph consume the returned data. Descriptions,
ports, ranges/defaults and structural flags continue to come from the descriptor.
An older/external descriptor without documentation displays its reference and an
explicit missing-example message rather than inventing a route.

Every built-in descriptor must have an authored example containing that node.
`cargo test -p pr0-core documentation` validates all examples through the same
server graph validator, including subgraph containment, channels and spectral
formats. Update/add examples when modifying a node's contract. Do not invent
project-specific sample IDs or claim that an unassigned physical input produces
sound. State the setup required to recreate the patch. Subgraph examples show
valid internal boundaries inside an included parent container.

`DocumentationGraph.vue` uses Vue Flow, `PatchNode.vue`, `SignalEdge.vue` and the
main graph's automatic layout function, measuring actual node sizes. It creates an
isolated flow store and explicitly supplies inactive/empty telemetry. Controls
are disabled; gears expose the example settings. Wires cannot request live signal
previews. Examples cannot edit or activate the user's project. Named send/receive
routes intentionally have no cable between the matching names; their target
values are included in the settings/connection reference.

## Contextual explanations

Use `HelpNote` beside a title/field heading for explanatory prose. Do not hide
errors, connection state, permissions required for the current action, or values
that the user needs to operate the interface. The global `i` shortcut/header
button expands or collapses descriptions in their surrounding content area, with
the preference saved locally. Info icons and their hovers remain available in both states. Typing in a field does not trigger the shortcut.

The shared helper supports pointer hover, keyboard focus/Enter/Space, and tapping.
It mounts inside the nearest dialog and uses a native popover so the explanation
is in the top layer without falling outside the modal's interactive subtree.
Older webviews without Popover API use fixed positioning inside the dialog.
Dismiss on Escape, outside input, anchor scrolling/resizing, or a global expansion change;
allow the pointer to move onto long explanations. Keep field names stable with
explicit accessible labels when placing help inside a form label.

Browser regression coverage is in `web/e2e/documentation.spec.ts`. It checks the
modal/top-layer behavior, keyboard/global preference, narrow-screen interaction,
setup sections, all catalog examples, and the absence of example mutations or
signal-preview requests. These are software/UI checks, not audio or device tests.

Expanded descriptions use a component-owned Teleport mount point after their
heading or field, outside form labels and heading names. Node controls occupy a
separate body so descriptions expand below them without shifting port positions.
