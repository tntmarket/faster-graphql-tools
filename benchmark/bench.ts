import { readFileSync } from 'fs';
import { dirname, join } from 'path';
import { Bench } from 'tinybench';
import { fileURLToPath } from 'url';
import { ParsedSchema } from '../index.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const PETS_SCHEMA = readFileSync(join(__dirname, '../testing/pets.schema.graphql'), 'utf-8');

const bench = new Bench({ time: 1000 });

const parsedSchema = new ParsedSchema(PETS_SCHEMA);

const simpleDocument = /* GraphQL */ `
    {
        animalOwner {
            name
            contactDetails {
                email
            }
        }
    }
`;

const complexDocument = /* GraphQL */ `
    {
        animalOwner {
            name
            age
            contactDetails {
                email
                phone
                address {
                    streetNumber
                    zip
                }
            }
        }
        pets {
            ... on Dog {
                name
                breed
            }
            ... on Cat {
                name
                favoriteMilkBrand
            }
            ... on Parrot {
                name
                wingSpan
            }
        }
        allSpecies {
            name
        }
    }
`;

bench
    .add('extractSchemaCoordinates - simple document', () => {
        parsedSchema.extractSchemaCoordinates(simpleDocument);
    })
    .add('extractSchemaCoordinates - complex document', () => {
        parsedSchema.extractSchemaCoordinates(complexDocument);
    })
    .add('extractSchemaCoordinates - with schema parsing', () => {
        const schema = new ParsedSchema(PETS_SCHEMA);
        schema.extractSchemaCoordinates(simpleDocument);
    });

await bench.run();

console.log('\nBenchmark Results:');
console.table(bench.table());

// Baseline performance metrics - fail if results regress below these thresholds
const PERFORMANCE_THRESHOLDS = {
    'extractSchemaCoordinates - simple document': {
        maxLatencyNs: 3990 * 1.1, // Allow 10% regression
        minThroughput: 252218 * 0.9, // Allow 10% regression
    },
    'extractSchemaCoordinates - complex document': {
        maxLatencyNs: 14289 * 1.1,
        minThroughput: 70389 * 0.9,
    },
    'extractSchemaCoordinates - with schema parsing': {
        maxLatencyNs: 36734 * 1.1,
        minThroughput: 27334 * 0.9,
    },
};

// Check for regressions
let hasRegression = false;
const regressions: string[] = [];

for (const task of bench.tasks) {
    const threshold = PERFORMANCE_THRESHOLDS[task.name as keyof typeof PERFORMANCE_THRESHOLDS];
    if (!threshold) continue;

    const result = task.result!;
    const latencyNs = result.latency.mean * 1_000_000; // Convert ms to ns
    const throughput = result.throughput.mean; // ops/s

    if (latencyNs > threshold.maxLatencyNs) {
        hasRegression = true;
        regressions.push(
            `❌ ${task.name}: Latency regression detected!\n` +
            `   Expected: ≤${threshold.maxLatencyNs.toFixed(0)} ns\n` +
            `   Actual: ${latencyNs.toFixed(0)} ns`
        );
    }

    if (throughput < threshold.minThroughput) {
        hasRegression = true;
        regressions.push(
            `❌ ${task.name}: Throughput regression detected!\n` +
            `   Expected: ≥${threshold.minThroughput.toFixed(0)} ops/s\n` +
            `   Actual: ${throughput.toFixed(0)} ops/s`
        );
    }
}

if (hasRegression) {
    console.error('\n⚠️  Performance Regression Detected:\n');
    regressions.forEach(msg => console.error(msg));
    process.exit(1);
} else {
    console.log('\n✅ All benchmarks passed performance thresholds');
} 
