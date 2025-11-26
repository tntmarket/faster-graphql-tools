# faster-graphql-tools

A Rust port of [extract-schema-coordinates](https://github.com/sharkcore/extract-schema-coordinates).

## Example Usage

```json
["Query.business", "Business.name", "Business.location", "Location.city"]
```

```js
import ParsedSchema from 'faster-graphql-tools';

const parsedSchema = new ParsedSchema(schemaText);

const fieldCoordinates = parsedSchema.extractSchemaCoordinates(`
    query GET_BUSINESS($BizId: String) {
        business(id: $BizId) {
            name
            location {
                city
            }
        }
    }
`);

// ["Query.business", "Business.name", "Business.location", "Location.city"]
```