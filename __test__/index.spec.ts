import test from 'ava';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { ParsedSchema } from '../index';

const __dirname = dirname(fileURLToPath(import.meta.url));
const PETS_SCHEMA = readFileSync(join(__dirname, '../testing/pets.schema.graphql'), 'utf-8');

test('process one document using native ParsedSchema', (t) => {
    const parsedSchema = new ParsedSchema(PETS_SCHEMA);

    const document = /* GraphQL */ `
        {
            animalOwner {
                name
                contactDetails {
                    email
                }
            }
        }
    `;

    const fieldCoordinates = parsedSchema.extractSchemaCoordinates(document);

    t.deepEqual([...fieldCoordinates].sort(), [
        'ContactDetails.email',
        'Human.contactDetails',
        'Human.name',
        'Root.animalOwner',
    ]);
})
