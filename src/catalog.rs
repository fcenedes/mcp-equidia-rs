use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use rmcp::model::{Tool, ToolAnnotations};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{Error, Result};

#[derive(Clone, Deserialize)]
pub(crate) struct Parameter {
    pub name: String,
    pub location: String,
    pub explode: bool,
    pub style: String,
}

#[derive(Clone, Deserialize)]
pub(crate) struct Operation {
    pub name: String,
    pub method: String,
    pub path: String,
    pub operation_id: String,
    pub tags: Vec<String>,
    pub description: String,
    pub parameters: Vec<Parameter>,
    pub input_schema: Value,
}

#[derive(Deserialize)]
struct Manifest {
    operations: Vec<Operation>,
    aliases: BTreeMap<String, String>,
}

pub(crate) struct Definition {
    pub tool: Tool,
    pub operation: Option<Operation>,
    validator: jsonschema::Validator,
}

impl Definition {
    pub fn validate(&self, args: &Value) -> Result<()> {
        if let Some(error) = self.validator.iter_errors(args).next() {
            return Err(Error::Input(format!(
                "invalid arguments at {} (constraint {}); inspect this tool with `schema`",
                error.instance_path(),
                error.schema_path()
            )));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct Catalog(Arc<Vec<Definition>>);

impl Catalog {
    pub fn new(filter: Option<&str>) -> Result<Self> {
        let manifest: Manifest = serde_json::from_str(include_str!("../api/catalog.json"))?;
        let mut definitions = Vec::new();
        for operation in &manifest.operations {
            if operation.method != "GET" {
                return Err(Error::Input(
                    "catalogue contains a non-read operation".into(),
                ));
            }
            definitions.push(definition(
                &operation.name,
                &operation.description,
                operation.input_schema.clone(),
                Some(operation.clone()),
            )?);
        }
        for (alias, target) in manifest.aliases {
            let op = manifest
                .operations
                .iter()
                .find(|o| o.name == target)
                .ok_or_else(|| Error::Input("invalid catalogue alias".into()))?;
            definitions.push(definition(
                &alias,
                &op.description,
                op.input_schema.clone(),
                Some(op.clone()),
            )?);
        }
        definitions.push(definition("equidia_api_catalog", "Search the offline catalogue of anonymous Equidia GET operations. Returns canonical tool names, paths, descriptions and parameter schemas. All operations are read-only and use no upstream credentials.",
            json!({"type":"object","properties":{"query":{"type":"string","maxLength":200}},"required":[],"additionalProperties":false}), None)?);
        if let Some(filter) = filter.map(str::trim).filter(|s| !s.is_empty() && *s != "*") {
            let names: Vec<_> = filter.split(',').map(str::trim).collect();
            if names
                .iter()
                .any(|n| !definitions.iter().any(|d| d.tool.name == *n))
            {
                return Err(Error::Input(
                    "EQUIDIA_ENABLED_TOOLS contains an unknown tool; run `tools`".into(),
                ));
            }
            definitions.retain(|d| names.contains(&d.tool.name.as_ref()));
        }
        Ok(Self(Arc::new(definitions)))
    }

    pub fn iter(&self) -> impl Iterator<Item = &Definition> {
        self.0.iter()
    }

    pub fn get(&self, name: &str) -> Result<&Definition> {
        self.iter()
            .find(|d| d.tool.name == name)
            .ok_or_else(|| Error::Input(format!("unknown or disabled tool: {name}")))
    }

    pub fn search(&self, query: &str) -> Value {
        let query = query.to_lowercase();
        let mut seen = BTreeSet::new();
        let operations: Vec<_> = self.iter().filter_map(|d| {
            let op = d.operation.as_ref()?;
            if !format!("{} {} {} {} {}", d.tool.name, op.name, op.path, op.description, op.tags.join(" ")).to_lowercase().contains(&query) || !seen.insert(&op.name) {
                return None;
            }
            Some(json!({"tool":d.tool.name,"canonicalTool":op.name,"method":op.method,"path":op.path,"operationId":op.operation_id,"tags":op.tags,"description":op.description,"inputSchema":op.input_schema}))
        }).collect();
        json!({"source":"https://apiv2.equidia.fr/api/docs.json","retrieved":"2026-11-11","operations":operations})
    }
}

fn definition(
    name: &str,
    description: &str,
    schema: Value,
    operation: Option<Operation>,
) -> Result<Definition> {
    let validator = jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .map_err(|_| Error::Input(format!("invalid built-in schema for {name}")))?;
    let properties = schema
        .as_object()
        .cloned()
        .ok_or_else(|| Error::Input("schema must be an object".into()))?;
    Ok(Definition {
        tool: Tool::new(name.to_owned(), description.to_owned(), properties).with_annotations(
            ToolAnnotations::new()
                .read_only(true)
                .destructive(false)
                .idempotent(true)
                .open_world(operation.is_some()),
        ),
        operation,
        validator,
    })
}
