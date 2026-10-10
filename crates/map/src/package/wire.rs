//! Strict package reader: unknown fields, other schemas and other generation
//! recipes are rejected rather than interpreted.
use super::*;
use serde::{Deserializer, de::Error};

macro_rules! strict_object {
    ($name:ident, $target:path, {$($(#[$attr:meta])* $field:ident: $ty:ty),* $(,)?}) => {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct $name { $($(#[$attr])* $field: $ty),* }
        impl From<$name> for $target {
            fn from(raw: $name) -> Self { Self {$($field: raw.$field),*} }
        }
    };
    ($name:ident, $reader:ident, $target:path, {$($fields:tt)*}) => {
        strict_object!($name, $target, {$($fields)*});
        pub(super) fn $reader<'de, D: Deserializer<'de>>(deserializer: D) -> Result<$target, D::Error> {
            $name::deserialize(deserializer).map(Into::into)
        }
    };
}
mod nested;

#[derive(Deserialize)]
#[serde(try_from = "u16")]
struct Schema(u16);
impl TryFrom<u16> for Schema {
    type Error = &'static str;
    fn try_from(schema: u16) -> Result<Self, Self::Error> {
        (schema == crate::MAP_SCHEMA_VERSION)
            .then_some(Self(schema))
            .ok_or("unsupported map package schema; regenerate the package")
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictPackage {
    schema_version: Schema,
    generator_version: u16,
    generation_recipe_version: u16,
    #[serde(deserialize_with = "nested::request")]
    request: MapRequest,
    #[serde(deserialize_with = "nested::estimate")]
    estimate: MapEstimate,
    #[serde(deserialize_with = "nested::source_locks")]
    source_locks: Vec<SourceLock>,
    #[serde(deserialize_with = "nested::projection")]
    projection: ProjectionMetadata,
    #[serde(deserialize_with = "nested::provenance")]
    provenance: EnvironmentalProvenance,
    #[serde(deserialize_with = "nested::environment")]
    environment: PreparedEnvironment,
    content_hash: [u8; 32],
}

impl<'de> Deserialize<'de> for MapPackage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = StrictPackage::deserialize(deserializer)?;
        if raw.generation_recipe_version != crate::GENERATION_RECIPE_VERSION {
            return Err(D::Error::custom(
                MapPackageError::InvalidGenerationRecipeVersion,
            ));
        }
        raw.environment.validate().map_err(D::Error::custom)?;
        Ok(Self {
            schema_version: raw.schema_version.0,
            generator_version: raw.generator_version,
            generation_recipe_version: raw.generation_recipe_version,
            request: raw.request,
            estimate: raw.estimate,
            source_locks: raw.source_locks,
            projection: raw.projection,
            provenance: raw.provenance,
            environment: raw.environment,
            content_hash: raw.content_hash,
        })
    }
}
