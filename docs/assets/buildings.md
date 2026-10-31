# DAT building art

`make assets-import-de AOE_ASSET_PACK=local-assets/packs/<base-hash>` imports
all 41 gameplay building records and the six spawned bridge segments in one
pass. The owner-licensed DE installation is mounted read-only; original pixels,
source digests and generated packs stay in ignored local storage. The current
base still supplies interface and remaining effect assets until their category
passes replace the trial import.

## One importer and one runtime recipe

The importer selects numeric gameplay unit IDs, reads their standing, dying,
corpse, construction, damage, snow, researching and research-completed graphics,
and follows every drawable delta and nested annex. The building tail's generic
destruction and rubble graphics are retained alongside architecture-specific
unit lifecycle graphics. Closed/open graphics share the DAT filename family;
there are no individual building image converters or hand-picked image lists.

A DE filename can identify a real SLD even when its classic SLP ID is -1.
Such layers are imported when installed. Explicitly absent negative-ID layers
remain unavailable metadata. Empty civilization slots are distinguished from
missing records; for example, one nested Town Center annex has a zero pointer.
Unknown nonempty references fail import. Legacy terrain-only graphic names
produce no SLD layer: the Farm's terrain textures belong to the terrain pass.
Particle references remain the effects pass's responsibility.

Each drawable SLD retains every native cell and direction, including trailing
source cells. The runtime addresses the DAT's declared timeline and selects
static building variants consistently by entity ID. Delta layers keep independent
fractional frame durations and replay delays. Native anchors, owner weights,
shadows, signed pixel offsets and annex tile displacements are preserved.
The renderer orders layers and annex depth without mirroring building art.
Bridge presentation retains its existing half-size world geometry; its native
pixel cells are unchanged.

Runtime building selection uses these DAT recipes alone. Missing recipe sections
stay absent. The completed-category startup path has retired its supplemental
building filename lists, sampled fish sheets and special farm crop window.

## Residency and lifecycle

Building timelines share the unit cache's four on-demand colour/mask page
pairs. Off-screen animations reserve no GPU rectangles. Current visible cells
are protected; unprotected cells are evicted by least recent use. The normal
atlas payload remains 256 MiB including the static base; the absolute renderer
cap remains 64 layers / 1 GiB. Geometry and visibility stay available before a
cell becomes resident, so eviction cannot erase future demand or picking bounds.
WebGPU, WebGL2 and Canvas consume the same frame selection and physical mapping.

Playback uses a millisecond clock and the DAT's fractional durations. Construction
progress selects the native construction angle; wall connectivity selects its
orientation. Damage overlays keep their complete native loops. On removal,
the completed building's age-specific destruction plays once, followed by three
seconds of its DAT corpse/rubble. Exhausted Farms leave no rubble. The scene
record stays 32 bytes; a bounded companion list carries fractional destruction
time from the authoritative presentation clock.

Farm crops remain terrain triangles. The complete DE crop sheet now uses the
generic native-cell terrain importer and view-dependent residency. Footprint preparation, dirt aprons, ordinary terrain
blending and server-owned construction rules are unchanged by the animation pass.

## Validation

`make test-assets-de` validates all selected building SLDs against the licensed
DAT in one inventory test. Portable tests cover bounded building tails, nested
references, native layers without classic IDs, full-cell selection, independent
replay timing, recipe overlay index offsets, native geometry before residency,
eviction, and destruction-to-rubble selection. The pressure fixture requires
large complete loops to fit through residency rather than startup atlas sampling.

Per-commit checks follow the owner's redirect: focused tests, `make test-wasm`,
`make lint`, `make structure-check` and `make perf-ci`. Shipping owns E2E and
video qualification. Synthetic tests do not establish finished RTS performance
or dedicated hardware qualification.
