# Geographic vegetation patches

The optional `vegetation_corrections` field on an ordinary overview worker
request accepts a bounded schema-1 `VegetationPatchDocument`. An absent field
is an empty document. Each document binds to the normalized 600 CE request,
its effective projected footprint, the actual overview vegetation grid axis,
and `potential-biome-nearest-gdal-0.19-patches-v1` preprocessing.

Every patch has a stable ID, a cited source, a local projected-metre rectangle,
an applicability interval containing 600 CE, an operation, and a priority.
Sources sort by ID. Patches sort by `(priority, id)`; the last applicable
patch covering a sample centre wins. The worker validates the entire document
before acquiring any source, including its 8 KiB JSON limit. This keeps the
combined history, water, and vegetation correction documents within the
worker's 64 KiB request envelope.

`historical_biome` sets a supported potential-natural-vegetation class;
`unknown` writes source class zero, which keeps procedural fallback explicit.
`modern_observation` is retained as cited context but never substitutes for
a year-600 reconstruction. No patch is generated automatically for a
demonstration. Corrections are applied before pyramid reduction. Their
canonical digest enters the vegetation source-lock preprocessing identity and
therefore the package identity; the layer reports historically corrected
provenance only when a historical or unknown operation is present.
