# World locator outline

`world-land.svg` is a lightweight visual backdrop for the geographic selection
control. It is not a terrain source or a reconstruction of coastlines in 600 CE.
No country borders, external requests, or game assets are included.

Source: [Natural Earth 1:110m land](https://www.naturalearthdata.com/downloads/110m-physical-vectors/),
[public domain](https://www.naturalearthdata.com/about/terms-of-use/).
The input was the Natural Earth vector repository's
[`v5.1.2` GeoJSON](https://raw.githubusercontent.com/nvkelso/natural-earth-vector/v5.1.2/geojson/ne_110m_land.geojson).
Its SHA-256 is
`9e0729ee253ca7d7a5c4ae9395fb1902264c5377c52e224d13dd85010e2835d9`.

Each Polygon/MultiPolygon ring is rendered in source order as an SVG subpath,
using equirectangular coordinates `x = longitude + 180`, `y = 90 - latitude`,
rounded to three decimal places. Rings retain their closure and use even-odd
fill to preserve holes. The 127 polygons use a 360×180 view box shared with the
selector grid and the server-projected selection boundary. The source GeoJSON
remains in the ignored cache; the distributable SVG is 80 KiB or less.
