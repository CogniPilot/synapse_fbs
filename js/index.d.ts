export * from './topic_catalog.js';

/** Absolute path to the directory containing the canonical `.fbs` schema files. */
export const fbsDir: string;

/** Absolute path to the directory containing the generated `.bfbs` reflection schemas. */
export const bfbsDir: string;

/** Schema file names shipped under {@link fbsDir}, in FlatBuffers include order. */
export const schemaFiles: readonly string[];

/**
 * Resolve the absolute path to a shipped `.fbs` schema file.
 * @param name Schema file name, e.g. `log.fbs`.
 * @returns Absolute filesystem path.
 */
export function schemaPath(name: string): string;
