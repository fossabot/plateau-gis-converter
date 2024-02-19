use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// TODO: Implement KHR_materials_variants
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Clone)]
pub struct KhrMaterialsVariants {
    #[serde(flatten)]
    pub others: HashMap<String, Value>,
}
