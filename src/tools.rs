use std::time::Instant;

use serde_json::Value;

use crate::{
    catalog::Catalog,
    client::{EquidiaClient, Result},
};

#[derive(Clone)]
pub(crate) struct EngineState {
    pub client: EquidiaClient,
    pub catalog: Catalog,
}

impl EngineState {
    pub async fn call(&self, name: &str, args: Value) -> Result<Value> {
        let definition = self.catalog.get(name)?;
        let start = Instant::now();
        let result = async {
            definition.validate(&args)?;
            match &definition.operation {
                Some(operation) => self.client.call(operation, &args).await,
                None => Ok(self.catalog.search(args["query"].as_str().unwrap_or(""))),
            }
        }
        .await;
        self.client
            .metrics
            .tool(name, result.is_ok(), start.elapsed());
        result
    }
}
