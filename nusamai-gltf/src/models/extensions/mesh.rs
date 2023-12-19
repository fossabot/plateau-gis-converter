use crate::models::texture_info::TextureInfo;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Feature ID in EXT_mesh_features
#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct FeatureId {
    /// The number of unique features in the attribute or texture.
    pub feature_count: u32,

    /// A value that indicates that no feature is associated with this vertex or texel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub null_feature_id: Option<u32>,

    /// A label assigned to this feature ID set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    /// An attribute containing feature IDs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribute: Option<u32>,

    /// A texture containing feature IDs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture: Option<FeatureIdTexture>,

    /// The index of the property table containing per-feature property values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_table: Option<u32>,

    /// JSON object with extension-specific objects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<HashMap<String, Value>>,

    /// Application-specific data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<Value>,
}

/// Feature ID Texture in EXT_mesh_features
#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct FeatureIdTexture {
    /// Texture channels containing feature IDs.
    #[serde(default = "default_channels")]
    pub channels: Vec<u32>,

    /// Refer to textureInfo.schema.json
    #[serde(flatten)]
    pub texture_info: Option<TextureInfo>,

    /// Additional properties (details not provided in the schema)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tex_coord: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<HashMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<Value>,
}

fn default_channels() -> Vec<u32> {
    vec![0]
}

/// EXT_mesh_features glTF Mesh Primitive extension
#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct Primitive {
    #[serde(skip_serializing_if = "Option::is_none", rename = "EXT_mesh_features")]
    pub ext_mesh_features: Option<MeshPrimitiveExtMeshFeatures>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "EXT_structural_metadata"
    )]
    pub ext_structural_metadata: Option<MeshPrimitiveExtStructuralMetadata>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "KHR_materials_variants"
    )]
    pub khr_materials_variants: Option<KhrMaterialsVariants>,
}

// TODO: Implement KHR_materials_variants
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct KhrMaterialsVariants {
    #[serde(flatten)]
    pub others: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct MeshPrimitiveExtMeshFeatures {
    /// An array of feature ID sets.
    pub feature_ids: Vec<FeatureId>,

    /// Additional properties (details not provided in the schema)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<HashMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<Value>,
}

/// EXT_structural_metadata glTF Mesh Primitive extension
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct MeshPrimitiveExtStructuralMetadata {
    /// An array of indexes of property textures in the root `EXT_structural_metadata` object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_textures: Option<Vec<u32>>,

    /// An array of indexes of property attributes in the root `EXT_structural_metadata` object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_attributes: Option<Vec<u32>>,

    /// JSON object with extension-specific objects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,

    /// Application-specific data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<Value>,
}
