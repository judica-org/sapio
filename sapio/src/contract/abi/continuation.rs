// Copyright Judica, Inc 2021
//
// This Source Code Form is subject to the terms of the Mozilla Public
//  License, v. 2.0. If a copy of the MPL was not distributed with this
//  file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ABI for contract resumption

use sapio_base::effects::EffectPath;
use sapio_base::serialization_helpers::SArc;
use schemars::schema::RootSchema;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{sync::Arc, collections::BTreeMap};
/// Instructions for how to resume a contract compilation at a given point
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct ContinuationPoint {
    /// The arguments required at this point
    /// TODO: De-Duplicate repeated types?
    pub schema: Option<SArc<RootSchema>>,
    /// The path at which this was compiled
    #[serde(serialize_with = "sapio_base::serialization_helpers::serializer")]
    #[serde(deserialize_with = "sapio_base::serialization_helpers::deserializer")]
    pub path: Arc<EffectPath>,
    /// Metadata for this particular Continuation Point
    pub simp: BTreeMap<i64, Value>,
}
impl ContinuationPoint {
    /// Creates a new continuation
    pub fn at(schema: Option<Arc<RootSchema>>, path: Arc<EffectPath>) -> Self {
        ContinuationPoint {
            schema: schema.map(SArc),
            path,
            simp: Default::default()
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use sapio_base::effects::PathFragment;
    #[test]
    fn test_continuation_point_ser() -> Result<(), Box<dyn std::error::Error>> {
        let a: ContinuationPoint = ContinuationPoint::at(
            Some(Arc::new(schemars::schema_for!(ContinuationPoint))),
            EffectPath::push(None, PathFragment::Named(SArc(Arc::new("one".into())))),
        );
        let b: ContinuationPoint = serde_json::from_str(&format!(
            "{{\"schema\":{},\"path\":\"one\"}}",
            serde_json::to_string(&schemars::schema_for!(ContinuationPoint))?
        ))?;
        assert_eq!(a, b);
        Ok(())
    }
}
