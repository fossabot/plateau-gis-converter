use std::collections::HashMap;

use ahash::RandomState;
use indexmap::IndexMap;

use nusamai_citygml::{
    schema::{TypeDef, TypeRef},
    Value,
};
use nusamai_gltf_json::extensions;

#[derive(Debug, Clone, Default)]
pub struct GltfPropertyType {
    pub class_name: String,
    pub property_name: String,
    pub class_property_type: extensions::gltf::ext_structural_metadata::ClassPropertyType,
    pub component_type:
        Option<extensions::gltf::ext_structural_metadata::ClassPropertyComponentType>,
}

// Attributes per vertex id
#[derive(Debug, Clone)]
pub struct FeatureAttributes {
    pub class_name: String,
    pub feature_id: u32,
    pub attributes: IndexMap<String, Value, RandomState>,
}

fn to_gltf_schema(type_ref: &TypeRef) -> GltfPropertyType {
    match type_ref {
        TypeRef::String => GltfPropertyType {
            class_property_type:
                extensions::gltf::ext_structural_metadata::ClassPropertyType::String,
            component_type: None,
            ..Default::default()
        },
        TypeRef::Integer => GltfPropertyType {
            class_property_type:
                extensions::gltf::ext_structural_metadata::ClassPropertyType::Scalar,
            component_type: Some(
                extensions::gltf::ext_structural_metadata::ClassPropertyComponentType::Int64,
            ),
            ..Default::default()
        },
        TypeRef::Double => GltfPropertyType {
            class_property_type:
                extensions::gltf::ext_structural_metadata::ClassPropertyType::Scalar,
            component_type: Some(
                extensions::gltf::ext_structural_metadata::ClassPropertyComponentType::Float64,
            ),
            ..Default::default()
        },
        TypeRef::Boolean => GltfPropertyType {
            class_property_type:
                extensions::gltf::ext_structural_metadata::ClassPropertyType::Boolean,
            component_type: None,
            ..Default::default()
        },
        TypeRef::Measure => GltfPropertyType {
            class_property_type:
                extensions::gltf::ext_structural_metadata::ClassPropertyType::Scalar,
            component_type: Some(
                extensions::gltf::ext_structural_metadata::ClassPropertyComponentType::Float64,
            ),
            ..Default::default()
        },
        TypeRef::Code => GltfPropertyType {
            class_property_type:
                extensions::gltf::ext_structural_metadata::ClassPropertyType::String,
            component_type: None,
            ..Default::default()
        },
        TypeRef::NonNegativeInteger => GltfPropertyType {
            class_property_type:
                extensions::gltf::ext_structural_metadata::ClassPropertyType::Scalar,
            component_type: Some(
                extensions::gltf::ext_structural_metadata::ClassPropertyComponentType::UInt64,
            ),
            ..Default::default()
        },
        TypeRef::JsonString => GltfPropertyType {
            class_property_type:
                extensions::gltf::ext_structural_metadata::ClassPropertyType::String,
            component_type: None,
            ..Default::default()
        },
        TypeRef::Point => GltfPropertyType {
            class_property_type: extensions::gltf::ext_structural_metadata::ClassPropertyType::Vec3,
            component_type: Some(
                extensions::gltf::ext_structural_metadata::ClassPropertyComponentType::Float64,
            ),
            ..Default::default()
        },
        TypeRef::Named(_) => GltfPropertyType {
            class_property_type:
                extensions::gltf::ext_structural_metadata::ClassPropertyType::String,
            component_type: None,
            ..Default::default()
        },
        TypeRef::URI => GltfPropertyType {
            class_property_type:
                extensions::gltf::ext_structural_metadata::ClassPropertyType::String,
            component_type: None,
            ..Default::default()
        },
        TypeRef::Date => GltfPropertyType {
            class_property_type:
                extensions::gltf::ext_structural_metadata::ClassPropertyType::String,
            component_type: None,
            ..Default::default()
        },
        TypeRef::DateTime => GltfPropertyType {
            class_property_type:
                extensions::gltf::ext_structural_metadata::ClassPropertyType::String,
            component_type: None,
            ..Default::default()
        },
        TypeRef::Unknown => todo!(),
    }
}

pub fn to_gltf_class(
    class_name: &str,
    type_def: &TypeDef,
) -> HashMap<String, extensions::gltf::ext_structural_metadata::Class> {
    let mut gltf_property_types = Vec::new();

    match type_def {
        TypeDef::Feature(f) => {
            for (name, attr) in &f.attributes {
                let mut property_type = to_gltf_schema(&attr.type_ref);
                property_type.class_name = class_name.to_string();
                property_type.property_name = name.clone();
                gltf_property_types.push(property_type);
            }
        }
        // todo: feature 以外の型も実装する
        TypeDef::Data(_) => unimplemented!(),
        TypeDef::Property(_) => unimplemented!(),
    }

    let mut class_properties = HashMap::new();
    for gltf_property_type in gltf_property_types.iter() {
        // Create Schema.classes
        class_properties.insert(
            gltf_property_type.property_name.clone(),
            extensions::gltf::ext_structural_metadata::ClassProperty {
                description: Some(gltf_property_type.property_name.clone()),
                type_: gltf_property_type.class_property_type.clone(),
                component_type: gltf_property_type.component_type.clone(),
                ..Default::default()
            },
        );
    }

    let mut class: HashMap<String, extensions::gltf::ext_structural_metadata::Class> =
        HashMap::new();
    class.insert(
        class_name.to_string(),
        extensions::gltf::ext_structural_metadata::Class {
            name: Some(class_name.to_string()),
            description: None,
            properties: class_properties.clone(),
            ..Default::default()
        },
    );

    class
}

pub fn to_gltf_property_table(
    class_name: &str,
    schema: &TypeDef,
    buffer_view_length: u32,
    feature_count: u32,
) -> (
    extensions::gltf::ext_structural_metadata::PropertyTable,
    u32,
) {
    // Create Schema.property_tables
    let mut property_table: extensions::gltf::ext_structural_metadata::PropertyTable =
        extensions::gltf::ext_structural_metadata::PropertyTable {
            class: class_name.to_string(),
            properties: HashMap::new(),
            count: feature_count,
            ..Default::default()
        };

    let mut buffer_view_length = buffer_view_length;
    match schema {
        TypeDef::Feature(f) => {
            for (name, attr) in &f.attributes {
                let property_type = to_gltf_schema(&attr.type_ref);
                // property_typeによって、PropertyTablePropertyの構造が変化する
                // todo: その他の型についても対応
                match property_type.class_property_type {
                    extensions::gltf::ext_structural_metadata::ClassPropertyType::String => {
                        property_table.properties.insert(
                            name.clone(),
                            extensions::gltf::ext_structural_metadata::PropertyTableProperty {
                                values: buffer_view_length,
                                string_offsets: Some(buffer_view_length + 1),
                                ..Default::default()
                            },
                        );
                        buffer_view_length += 2;
                    }
                    extensions::gltf::ext_structural_metadata::ClassPropertyType::Scalar => {
                        property_table.properties.insert(
                            name.clone(),
                            extensions::gltf::ext_structural_metadata::PropertyTableProperty {
                                values: buffer_view_length,
                                ..Default::default()
                            },
                        );
                        buffer_view_length += 1;
                    }
                    extensions::gltf::ext_structural_metadata::ClassPropertyType::Boolean => {
                        property_table.properties.insert(
                            name.clone(),
                            extensions::gltf::ext_structural_metadata::PropertyTableProperty {
                                values: buffer_view_length,
                                ..Default::default()
                            },
                        );
                        buffer_view_length += 1;
                    }
                    _ => unimplemented!(),
                }
            }
        }
        // todo: feature 以外の型も実装する
        TypeDef::Data(_) => unimplemented!(),
        TypeDef::Property(_) => unimplemented!(),
    }

    (property_table, buffer_view_length)
}

#[cfg(test)]
mod tests {
    use nusamai_citygml::schema::FeatureTypeDef;

    use super::*;

    #[test]
    fn test_to_gltf_schema() {
        let type_ref = TypeRef::String;
        let gltf_property_type = to_gltf_schema(&type_ref);
        assert_eq!(
            gltf_property_type.class_property_type,
            extensions::gltf::ext_structural_metadata::ClassPropertyType::String
        );

        let type_ref = TypeRef::Integer;
        let gltf_property_type = to_gltf_schema(&type_ref);
        assert_eq!(
            gltf_property_type.class_property_type,
            extensions::gltf::ext_structural_metadata::ClassPropertyType::Scalar
        );
        assert_eq!(
            gltf_property_type.component_type,
            Some(extensions::gltf::ext_structural_metadata::ClassPropertyComponentType::Int64)
        );

        let type_ref = TypeRef::Double;
        let gltf_property_type = to_gltf_schema(&type_ref);
        assert_eq!(
            gltf_property_type.class_property_type,
            extensions::gltf::ext_structural_metadata::ClassPropertyType::Scalar
        );
        assert_eq!(
            gltf_property_type.component_type,
            Some(extensions::gltf::ext_structural_metadata::ClassPropertyComponentType::Float64)
        );

        let type_ref = TypeRef::Boolean;
        let gltf_property_type = to_gltf_schema(&type_ref);
        assert_eq!(
            gltf_property_type.class_property_type,
            extensions::gltf::ext_structural_metadata::ClassPropertyType::Boolean
        );

        let type_ref = TypeRef::Measure;
        let gltf_property_type = to_gltf_schema(&type_ref);
        assert_eq!(
            gltf_property_type.class_property_type,
            extensions::gltf::ext_structural_metadata::ClassPropertyType::Scalar
        );
        assert_eq!(
            gltf_property_type.component_type,
            Some(extensions::gltf::ext_structural_metadata::ClassPropertyComponentType::Float64)
        );
    }

    #[test]
    fn test_to_gltf_classes() {
        let class_name = "Building".to_string();
        let attribute = TypeRef::String;
        let mut attributes: IndexMap<String, nusamai_citygml::schema::Attribute, RandomState> =
            IndexMap::default();

        attributes.insert(
            class_name.clone(),
            nusamai_citygml::schema::Attribute {
                type_ref: attribute,
                ..Default::default()
            },
        );

        let feature_type_def = TypeDef::Feature(FeatureTypeDef {
            attributes,
            ..Default::default()
        });

        let classes = to_gltf_class(&class_name, &feature_type_def);
        assert_eq!(classes.len(), 1);
    }

    #[test]
    fn test_to_gltf_property_tables() {
        let class_name = "Building".to_string();
        let attribute = TypeRef::String;
        let mut attributes: IndexMap<String, nusamai_citygml::schema::Attribute, RandomState> =
            IndexMap::default();

        attributes.insert(
            class_name.clone(),
            nusamai_citygml::schema::Attribute {
                type_ref: attribute,
                ..Default::default()
            },
        );

        let feature_type_def = TypeDef::Feature(FeatureTypeDef {
            attributes,
            ..Default::default()
        });

        let property_tables = to_gltf_property_table(&class_name, &feature_type_def, 0, 1);
        assert_eq!(property_tables.0.properties.len(), 1);
    }
}
