#![deny(clippy::all)]

use graphql_parser::{query, schema};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// A parsed GraphQL schema that can be reused to extract coordinates from multiple documents
#[napi]
pub struct ParsedSchema {
    type_map: Arc<HashMap<String, TypeInfo>>,
}

#[napi]
impl ParsedSchema {
    /// Create a new ParsedSchema from a schema string
    #[napi(constructor)]
    pub fn new(schema_text: String) -> Result<Self> {
        // Parse the schema
        let schema_doc = schema::parse_schema::<String>(&schema_text)
            .map_err(|e| Error::from_reason(format!("Failed to parse schema: {}", e)))?;

        // Build type map and wrap in Arc
        let type_map = Arc::new(build_type_map(&schema_doc));

        Ok(ParsedSchema { type_map })
    }

    /// Extract schema coordinates from a document using this parsed schema
    #[napi]
    pub fn extract_schema_coordinates(&self, document_text: String) -> Result<Vec<String>> {
        let mut coordinates = HashSet::new();

        // Parse the document
        let query_doc = query::parse_query::<String>(&document_text)
            .map_err(|e| Error::from_reason(format!("Failed to parse document: {}", e)))?;

        // Extract coordinates from the document
        for definition in &query_doc.definitions {
            match definition {
                query::Definition::Operation(operation) => {
                    extract_from_operation(
                        operation,
                        &self.type_map,
                        &query_doc,
                        &mut coordinates,
                    )?;
                }
                query::Definition::Fragment(_fragment) => {
                    // Fragments are processed when referenced in operations
                    continue;
                }
            }
        }

        let result: Vec<String> = coordinates.into_iter().collect();

        Ok(result)
    }
}

fn build_type_map(schema_doc: &schema::Document<'_, String>) -> HashMap<String, TypeInfo> {
    let mut type_map = HashMap::new();
    let mut query_type = "Query".to_string();
    let mut mutation_type = "Mutation".to_string();

    // Find the schema definition to get root operation types
    for definition in &schema_doc.definitions {
        if let schema::Definition::SchemaDefinition(schema_def) = definition {
            if let Some(type_def) = &schema_def.query {
                query_type = type_def.to_string();
            }
            if let Some(type_def) = &schema_def.mutation {
                mutation_type = type_def.to_string();
            }
        }
    }

    // Build the type map
    for definition in &schema_doc.definitions {
        match definition {
            schema::Definition::TypeDefinition(type_def) => {
                process_type_definition(type_def, &mut type_map);
            }
            schema::Definition::TypeExtension(type_ext) => {
                process_type_extension(type_ext, &mut type_map);
            }
            _ => {}
        }
    }

    // Create aliases for Query and Mutation to map to the actual schema types
    if query_type != "Query" {
        let query_fields = type_map
            .get(&query_type)
            .map(|t| t.fields.clone())
            .unwrap_or_default();
        type_map.insert(
            "Query".to_string(),
            TypeInfo {
                name: query_type.clone(),
                fields: query_fields,
            },
        );
    }

    if mutation_type != "Mutation" {
        let mutation_fields = type_map
            .get(&mutation_type)
            .map(|t| t.fields.clone())
            .unwrap_or_default();
        type_map.insert(
            "Mutation".to_string(),
            TypeInfo {
                name: mutation_type.clone(),
                fields: mutation_fields,
            },
        );
    }

    type_map
}

fn process_type_definition(
    type_def: &schema::TypeDefinition<'_, String>,
    type_map: &mut HashMap<String, TypeInfo>,
) {
    match type_def {
        schema::TypeDefinition::Object(obj) => {
            let mut fields = HashMap::new();
            for field in &obj.fields {
                fields.insert(field.name.to_string(), get_field_type(&field.field_type));
            }
            type_map.insert(
                obj.name.to_string(),
                TypeInfo {
                    name: obj.name.to_string(),
                    fields,
                },
            );
        }
        schema::TypeDefinition::Interface(iface) => {
            let mut fields = HashMap::new();
            for field in &iface.fields {
                fields.insert(field.name.to_string(), get_field_type(&field.field_type));
            }
            type_map.insert(
                iface.name.to_string(),
                TypeInfo {
                    name: iface.name.to_string(),
                    fields,
                },
            );
        }
        schema::TypeDefinition::InputObject(input) => {
            type_map.insert(
                input.name.to_string(),
                TypeInfo {
                    name: input.name.to_string(),
                    fields: HashMap::new(),
                },
            );
        }
        _ => {}
    }
}

fn process_type_extension(
    type_ext: &schema::TypeExtension<'_, String>,
    type_map: &mut HashMap<String, TypeInfo>,
) {
    match type_ext {
        schema::TypeExtension::Object(obj) => {
            let entry = type_map
                .entry(obj.name.to_string())
                .or_insert_with(|| TypeInfo {
                    name: obj.name.to_string(),
                    fields: HashMap::new(),
                });
            for field in &obj.fields {
                entry
                    .fields
                    .insert(field.name.to_string(), get_field_type(&field.field_type));
            }
        }
        _ => {}
    }
}

fn get_field_type(field_type: &schema::Type<'_, String>) -> String {
    match field_type {
        schema::Type::NamedType(name) => name.to_string(),
        schema::Type::NonNullType(inner) => get_field_type(inner),
        schema::Type::ListType(inner) => get_field_type(inner),
    }
}

fn extract_from_operation(
    operation: &query::OperationDefinition<String>,
    type_map: &Arc<HashMap<String, TypeInfo>>,
    query_doc: &query::Document<String>,
    coordinates: &mut HashSet<String>,
) -> Result<()> {
    let (root_type, selection_set, variable_defs) = match operation {
        query::OperationDefinition::Query(q) => {
            ("Query", &q.selection_set, &q.variable_definitions)
        }
        query::OperationDefinition::Mutation(m) => {
            ("Mutation", &m.selection_set, &m.variable_definitions)
        }
        query::OperationDefinition::Subscription(_) => {
            return Err(Error::from_reason(
                "Schema is not configured to execute subscription",
            ));
        }
        query::OperationDefinition::SelectionSet(ss) => ("Query", ss, &Vec::new()),
    };

    // Extract input types from variable definitions
    for var_def in variable_defs {
        extract_input_types(&var_def.var_type, type_map, coordinates);
    }

    // Extract coordinates from selection set
    extract_from_selection_set(
        &selection_set.items,
        root_type,
        type_map,
        query_doc,
        coordinates,
    );

    Ok(())
}

fn extract_input_types(
    var_type: &query::Type<String>,
    type_map: &Arc<HashMap<String, TypeInfo>>,
    coordinates: &mut HashSet<String>,
) {
    match var_type {
        query::Type::NamedType(name) => {
            // Only add if it's an input type (exists in type map and not a scalar)
            if type_map.contains_key(name) && !is_scalar(name) {
                coordinates.insert(name.to_string());
            }
        }
        query::Type::NonNullType(inner) => {
            extract_input_types(inner, type_map, coordinates);
        }
        query::Type::ListType(inner) => {
            extract_input_types(inner, type_map, coordinates);
        }
    }
}

fn is_scalar(type_name: &str) -> bool {
    matches!(type_name, "String" | "Int" | "Float" | "Boolean" | "ID")
}

fn extract_from_selection_set(
    selection_set: &[query::Selection<String>],
    parent_type: &str,
    type_map: &Arc<HashMap<String, TypeInfo>>,
    query_doc: &query::Document<String>,
    coordinates: &mut HashSet<String>,
) {
    for selection in selection_set {
        match selection {
            query::Selection::Field(field) => {
                // Get the actual type name from the schema if this is an alias (like Query -> Root)
                let actual_parent_type = type_map
                    .get(parent_type)
                    .map(|info| info.name.as_str())
                    .unwrap_or(parent_type);

                // Add the coordinate
                let coordinate = format!("{}.{}", actual_parent_type, field.name);
                coordinates.insert(coordinate);

                // Get the field type from the schema
                let field_type = type_map
                    .get(parent_type)
                    .and_then(|type_info| type_info.fields.get(&field.name))
                    .map(|s| s.as_str());

                // If field has selections, traverse them
                if !field.selection_set.items.is_empty() {
                    if let Some(field_type_name) = field_type {
                        extract_from_selection_set(
                            &field.selection_set.items,
                            field_type_name,
                            type_map,
                            query_doc,
                            coordinates,
                        );
                    }
                    // If field doesn't exist in schema, we don't traverse its children
                    // This handles the case of non-existent fields with children
                }
            }
            query::Selection::FragmentSpread(spread) => {
                // Find the fragment definition
                for definition in &query_doc.definitions {
                    if let query::Definition::Fragment(fragment) = definition {
                        if fragment.name == spread.fragment_name {
                            let fragment_type = match &fragment.type_condition {
                                query::TypeCondition::On(type_name) => type_name.as_str(),
                            };
                            extract_from_selection_set(
                                &fragment.selection_set.items,
                                fragment_type,
                                type_map,
                                query_doc,
                                coordinates,
                            );
                        }
                    }
                }
            }
            query::Selection::InlineFragment(inline) => {
                let fragment_type = match &inline.type_condition {
                    Some(query::TypeCondition::On(type_name)) => type_name.as_str(),
                    None => parent_type,
                };
                extract_from_selection_set(
                    &inline.selection_set.items,
                    fragment_type,
                    type_map,
                    query_doc,
                    coordinates,
                );
            }
        }
    }
}

#[derive(Debug, Clone)]
struct TypeInfo {
    name: String,
    fields: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const PETS_SCHEMA: &str = include_str!("../testing/pets.schema.graphql");

    fn extract_and_sort(document: &str, schema: &str) -> Vec<String> {
        let parsed_schema = ParsedSchema::new(schema.to_string()).expect("Should parse schema");
        let mut result = parsed_schema
            .extract_schema_coordinates(document.to_string())
            .expect("Should extract schema coordinates");
        result.sort();
        result
    }

    #[test]
    fn test_basic_query() {
        let document = r#"
            {
                animalOwner {
                    name
                    contactDetails {
                        email
                    }
                }
            }
        "#;

        let result = extract_and_sort(document, PETS_SCHEMA);
        assert_eq!(
            result,
            vec![
                "ContactDetails.email",
                "Human.contactDetails",
                "Human.name",
                "Root.animalOwner",
            ]
        );
    }

    #[test]
    fn test_basic_mutation() {
        let document = r#"
            mutation {
                addCat(name: "Palmerston") {
                    name
                    favoriteMilkBrand
                }
            }
        "#;

        let result = extract_and_sort(document, PETS_SCHEMA);
        assert_eq!(
            result,
            vec!["Cat.favoriteMilkBrand", "Cat.name", "Mutation.addCat"]
        );
    }

    #[test]
    fn test_extended_types() {
        let document = r#"
            {
                animalOwner {
                    name
                    contactDetails {
                        email
                        address {
                            zip
                        }
                    }
                }
            }
        "#;

        let result = extract_and_sort(document, PETS_SCHEMA);
        assert_eq!(
            result,
            vec![
                "Address.zip",
                "ContactDetails.address",
                "ContactDetails.email",
                "Human.contactDetails",
                "Human.name",
                "Root.animalOwner",
            ]
        );
    }

    #[test]
    fn test_multiple_operations() {
        let document = r#"
            {
                animalOwner {
                    name
                }
            }
            {
                animalOwner {
                    contactDetails {
                        email
                    }
                }
            }
        "#;

        let result = extract_and_sort(document, PETS_SCHEMA);
        assert_eq!(
            result,
            vec![
                "ContactDetails.email",
                "Human.contactDetails",
                "Human.name",
                "Root.animalOwner",
            ]
        );
    }

    #[test]
    fn test_includes_non_existent_fields_as_leaf_nodes() {
        let document = r#"
            {
                animalOwner {
                    name
                    I_DONT_EXIST
                    contactDetails {
                        email
                        I_DONT_EXIST
                    }
                }
            }
        "#;

        let result = extract_and_sort(document, PETS_SCHEMA);
        assert_eq!(
            result,
            vec![
                "ContactDetails.I_DONT_EXIST",
                "ContactDetails.email",
                "Human.I_DONT_EXIST",
                "Human.contactDetails",
                "Human.name",
                "Root.animalOwner",
            ]
        );
    }

    #[test]
    fn test_includes_non_existent_fields_as_non_leaf_nodes() {
        let document = r#"
            {
                animalOwner {
                    name
                    contactDetails {
                        email
                        I_DONT_EXIST {
                            foo
                            bar
                        }
                    }
                }
            }
        "#;

        let result = extract_and_sort(document, PETS_SCHEMA);
        assert_eq!(
            result,
            vec![
                "ContactDetails.I_DONT_EXIST",
                "ContactDetails.email",
                "Human.contactDetails",
                "Human.name",
                "Root.animalOwner",
            ]
        );
    }

    #[test]
    fn test_fragments() {
        let document = r#"
            {
                animalOwner {
                    name
                }
                allSpecies {
                    ...doggoDetails
                    ...catFacts
                }
                pets {
                    ...parrotParticulars
                }
            }

            fragment doggoDetails on Dog {
                breed
            }

            fragment catFacts on Cat {
                favoriteMilkBrand
                name
            }

            fragment parrotParticulars on Parrot {
                wingSpan
            }
        "#;

        let result = extract_and_sort(document, PETS_SCHEMA);
        assert_eq!(
            result,
            vec![
                "Cat.favoriteMilkBrand",
                "Cat.name",
                "Dog.breed",
                "Human.name",
                "Parrot.wingSpan",
                "Root.allSpecies",
                "Root.animalOwner",
                "Root.pets",
            ]
        );
    }

    #[test]
    fn test_fragments_with_interface_fields() {
        let document = r#"
            {
                animalOwner {
                    name
                }
                allSpecies {
                    name
                    ...doggoDetails
                }
            }

            fragment doggoDetails on Dog {
                breed
                name
            }
        "#;

        let result = extract_and_sort(document, PETS_SCHEMA);
        assert_eq!(
            result,
            vec![
                "Animal.name",
                "Dog.breed",
                "Dog.name",
                "Human.name",
                "Root.allSpecies",
                "Root.animalOwner",
            ]
        );
    }

    #[test]
    fn test_inline_fragments() {
        let document = r#"
            {
                animalOwner {
                    name
                }
                allSpecies {
                    ... on Dog {
                        breed
                    }
                    ... on Cat {
                        favoriteMilkBrand
                        name
                    }
                }
                pets {
                    ... on Parrot {
                        wingSpan
                    }
                }
            }
        "#;

        let result = extract_and_sort(document, PETS_SCHEMA);
        assert_eq!(
            result,
            vec![
                "Cat.favoriteMilkBrand",
                "Cat.name",
                "Dog.breed",
                "Human.name",
                "Parrot.wingSpan",
                "Root.allSpecies",
                "Root.animalOwner",
                "Root.pets",
            ]
        );
    }

    #[test]
    fn test_inline_fragments_with_interface_fields() {
        let document = r#"
            {
                animalOwner {
                    name
                }
                allSpecies {
                    name
                    ... on Dog {
                        breed
                        name
                    }
                }
            }
        "#;

        let result = extract_and_sort(document, PETS_SCHEMA);
        assert_eq!(
            result,
            vec![
                "Animal.name",
                "Dog.breed",
                "Dog.name",
                "Human.name",
                "Root.allSpecies",
                "Root.animalOwner",
            ]
        );
    }

    #[test]
    fn test_inline_fragments_without_type_condition() {
        let document = r#"
            query Foo($expandedInfo: Boolean) {
                allSpecies {
                    ... @include(if: $expandedInfo) {
                        name
                    }
                }
            }
        "#;

        let result = extract_and_sort(document, PETS_SCHEMA);
        assert_eq!(result, vec!["Animal.name", "Root.allSpecies"]);
    }

    #[test]
    fn test_copes_with_types_that_dont_exist_in_schema() {
        let document = r#"
            {
                allSpecies {
                    name
                    ... on Snake {
                        skin {
                            color
                        }
                    }
                }
            }
        "#;

        let result = extract_and_sort(document, PETS_SCHEMA);
        assert_eq!(result, vec!["Animal.name", "Root.allSpecies", "Snake.skin"]);
    }

    #[test]
    fn test_shows_inputs() {
        let document = r#"
            mutation AddVet($vetInfo: VetDetailsInput!, $somethingElse: String!) {
                addVet(details: $vetInfo)
            }
        "#;

        let result = extract_and_sort(document, PETS_SCHEMA);
        assert_eq!(result, vec!["Mutation.addVet", "VetDetailsInput"]);
    }

    #[test]
    fn test_mutation_with_arguments() {
        let document = r#"
            mutation AddCat($name: String) {
                addCat(name: $name) {
                    name
                }
            }
        "#;

        let result = extract_and_sort(document, PETS_SCHEMA);
        assert_eq!(result, vec!["Cat.name", "Mutation.addCat"]);
    }

    #[test]
    #[should_panic(expected = "Schema is not configured to execute subscription")]
    fn test_throws_error_on_unsupported_operation_types() {
        let document = r#"
            subscription Foo {
                bar
            }
        "#;

        let _ = extract_and_sort(document, PETS_SCHEMA);
    }
}

