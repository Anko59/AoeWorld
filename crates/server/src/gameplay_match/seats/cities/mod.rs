//! GeoNames place names for geographic starts. The gazetteer stays local;
//! the selected label travels with the seat through the match and its save.
use aoe_core::PlayerId;
use aoe_map::{MapEstimate, MapRequest};
use aoe_simulation::{BuildingKind, GameWorld};
use std::{collections::BTreeMap, io::BufRead, path::Path, sync::OnceLock};

#[cfg(test)]
mod tests;

const EARTH_METERS: f64 = 6_371_008.8;

struct City {
    id: u32,
    name: String,
    direction: [f64; 3],
}

fn direction(lat: f64, lon: f64) -> [f64; 3] {
    [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()]
}

fn read(input: impl BufRead) -> std::io::Result<Vec<City>> {
    let mut cities = Vec::new();
    for line in input.lines() {
        let line = line?;
        let mut fields = line.split('\t');
        let id = fields.next().and_then(|text| text.parse().ok());
        let name = fields.next().unwrap_or_default();
        // Skip ASCII name and alternate names, then read WGS84 coordinates.
        let lat = fields.nth(2).and_then(|text| text.parse::<f64>().ok());
        let lon = fields.next().and_then(|text| text.parse::<f64>().ok());
        let class = fields.next();
        let code = fields.next();
        if let (Some(id), Some(lat), Some(lon), Some("P"), Some(code)) = (id, lat, lon, class, code)
            && (-90.0..=90.0).contains(&lat)
            && (-180.0..=180.0).contains(&lon)
            && !matches!(code, "PPLH" | "PPLQ" | "PPLW")
            && aoe_protocol::valid_diplomacy_name(name)
        {
            cities.push(City {
                id,
                name: name.to_owned(),
                direction: direction(lat.to_radians(), lon.to_radians()),
            });
        }
    }
    Ok(cities)
}

fn load(path: &Path) -> Result<Vec<City>, crate::map_store::MapStoreError> {
    let bytes = crate::map_store::read_bounded_file(path, 64 * 1024 * 1024, "city gazetteer")?;
    Ok(read(bytes.as_slice())?)
}

struct MapCoordinates {
    request: MapRequest,
    side_meters: f64,
    side_tiles: f64,
}

impl MapCoordinates {
    /// Invert the local azimuthal equidistant grid on the mean Earth sphere.
    /// World Y points south. A dot product then ranks great-circle distances.
    fn location(&self, x: f64, y: f64) -> [f64; 3] {
        let lat = (f64::from(self.request.center_latitude_e7) / 1e7).to_radians();
        let lon = (f64::from(self.request.center_longitude_e7) / 1e7).to_radians();
        let east = (x / self.side_tiles - 0.5) * self.side_meters;
        let north = (0.5 - y / self.side_tiles) * self.side_meters;
        let distance = east.hypot(north) / EARTH_METERS;
        let bearing = east.atan2(north);
        let (sin_distance, cos_distance) = distance.sin_cos();
        let latitude = (lat.sin() * cos_distance + lat.cos() * sin_distance * bearing.cos())
            .clamp(-1.0, 1.0)
            .asin();
        let longitude = lon
            + (bearing.sin() * sin_distance * lat.cos())
                .atan2(cos_distance - lat.sin() * latitude.sin());
        direction(latitude, longitude)
    }
}

fn nearest(atlas: &[City], at: [f64; 3]) -> Option<&str> {
    let score = |city: &City| {
        at[0] * city.direction[0] + at[1] * city.direction[1] + at[2] * city.direction[2]
    };
    atlas
        .iter()
        .map(|city| (score(city), city))
        .max_by(|(a, left), (b, right)| a.total_cmp(b).then_with(|| right.id.cmp(&left.id)))
        .map(|(_, city)| city.name.as_str())
}

pub(in crate::gameplay_match) fn names(
    world: &GameWorld,
    request: MapRequest,
    estimate: MapEstimate,
    humans: &[bool],
) -> BTreeMap<PlayerId, String> {
    static ATLAS: OnceLock<Vec<City>> = OnceLock::new();
    if humans.iter().all(|&human| human) {
        return BTreeMap::new();
    }
    let atlas = ATLAS.get_or_init(
        || match load(Path::new("local-assets/geodata/cities.txt")) {
            Ok(cities) => cities,
            Err(error) => {
                tracing::warn!(%error, "city gazetteer unavailable; using numbered seat labels");
                Vec::new()
            }
        },
    );
    if atlas.is_empty() {
        return BTreeMap::new();
    }
    let map = MapCoordinates {
        request,
        side_meters: estimate.effective_side_meters as f64,
        side_tiles: estimate.tiles_per_side as f64,
    };
    let mut names = BTreeMap::new();
    for building in world
        .buildings()
        .filter(|b| b.kind == BuildingKind::TownCenter && b.is_complete())
    {
        if humans.get(usize::from(building.player.0)) != Some(&false) {
            continue;
        }
        let rect = building.footprint;
        let x = f64::from(rect.min.x + rect.max.x) / 2.0;
        let y = f64::from(rect.min.y + rect.max.y) / 2.0;
        if let Some(name) = nearest(atlas, map.location(x, y)) {
            names
                .entry(building.player)
                .or_insert_with(|| name.to_owned());
        }
    }
    names
}
