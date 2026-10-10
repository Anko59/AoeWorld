//! Schema-10 strictness must never silently fall back to the permissive legacy
//! reader. Untagged serde Content retains duplicate keys, unlike a JSON Value.
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
struct LandscapeSchema(u16);
impl TryFrom<u16> for LandscapeSchema {
    type Error = &'static str;
    fn try_from(schema: u16) -> Result<Self, Self::Error> {
        (schema == crate::LANDSCAPE_MAP_SCHEMA_VERSION)
            .then_some(Self(schema))
            .ok_or("expected landscape package schema")
    }
}

#[derive(Deserialize)]
#[serde(try_from = "u16")]
struct LegacySchema(u16);
impl TryFrom<u16> for LegacySchema {
    type Error = &'static str;
    fn try_from(schema: u16) -> Result<Self, Self::Error> {
        // Other previously parseable but unsupported numbers still fail validate,
        // not this legacy serde boundary. Only strict schema 10 cannot fall back.
        (schema != crate::LANDSCAPE_MAP_SCHEMA_VERSION)
            .then_some(Self(schema))
            .ok_or("landscape schema cannot use legacy reader")
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictPackage {
    schema_version: LandscapeSchema,
    generator_version: u16,
    #[serde(default = "legacy_generation_recipe_version")]
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

#[derive(Deserialize)]
struct LegacyPackage {
    schema_version: LegacySchema,
    generator_version: u16,
    #[serde(default = "legacy_generation_recipe_version")]
    generation_recipe_version: u16,
    request: MapRequest,
    estimate: MapEstimate,
    source_locks: Vec<SourceLock>,
    projection: ProjectionMetadata,
    provenance: EnvironmentalProvenance,
    environment: PreparedEnvironment,
    content_hash: [u8; 32],
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PackageWire {
    Landscape(StrictPackage),
    Legacy(LegacyPackage),
}

impl<'de> Deserialize<'de> for MapPackage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let package = match PackageWire::deserialize(deserializer)? {
            PackageWire::Landscape(raw) => Self {
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
            },
            PackageWire::Legacy(raw) => Self {
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
            },
        };
        if package.schema_version == crate::LANDSCAPE_MAP_SCHEMA_VERSION {
            profile::validate(
                package.schema_version,
                package.request.detail_profile,
                package.generation_recipe_version,
                package.environment.hydrology_evidence.is_some(),
            )
            .map_err(D::Error::custom)?;
            package
                .environment
                .validate_for_profile(package.request.detail_profile)
                .map_err(D::Error::custom)?;
        }
        Ok(package)
    }
}
