// Entry point for the @cognipilot/synapse-fbs schema-assets package.
//
// This package ships the canonical Synapse FlatBuffers schemas (`fbs/`) and the
// generated reflection schemas (`bfbs/`). It intentionally does not ship
// generated language bindings or depend on a `flatbuffers` runtime: the npm
// runtime release cadence does not track the pinned `flatc` version, so JS
// consumers generate or bring their own bindings from these assets.
import { fileURLToPath } from 'node:url';

const packageRoot = new URL('./', import.meta.url);

/** Absolute path to the directory containing the canonical `.fbs` schema files. */
export const fbsDir = fileURLToPath(new URL('fbs/', packageRoot));

/** Absolute path to the directory containing the generated `.bfbs` reflection schemas. */
export const bfbsDir = fileURLToPath(new URL('bfbs/', packageRoot));

/** Schema file names shipped under {@link fbsDir}, in FlatBuffers include order. */
export const schemaFiles = Object.freeze([
  'synapse_topics.fbs',
  'synapse_optical_flow.fbs',
  'synapse_mocap.fbs',
  'synapse_log.fbs',
  'synapse_sil.fbs',
  'synapse_all.fbs'
]);

/**
 * Resolve the absolute path to a shipped `.fbs` schema file.
 * @param {string} name Schema file name, e.g. `synapse_log.fbs`.
 * @returns {string} Absolute filesystem path.
 */
export function schemaPath(name) {
  return fileURLToPath(new URL(`fbs/${name}`, packageRoot));
}
