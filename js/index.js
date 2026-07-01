// Entry point for the @cognipilot/synapse-fbs schema-assets package.
//
// This package ships the canonical Synapse FlatBuffers schemas (`fbs/`) and the
// generated reflection schemas (`bfbs/`). It intentionally does not ship
// generated language bindings or depend on a `flatbuffers` runtime: the npm
// runtime release cadence does not track the pinned `flatc` version, so JS
// consumers generate or bring their own bindings from these assets.
import { fileURLToPath } from 'node:url';
export * from './topic_catalog.js';

const packageRoot = new URL('./', import.meta.url);

/** Absolute path to the directory containing the canonical `.fbs` schema files. */
export const fbsDir = fileURLToPath(new URL('fbs/', packageRoot));

/** Absolute path to the directory containing the generated `.bfbs` reflection schemas. */
export const bfbsDir = fileURLToPath(new URL('bfbs/', packageRoot));

/** Schema file names shipped under {@link fbsDir}, in FlatBuffers include order. */
export const schemaFiles = Object.freeze([
  'types.fbs',
  'sensors.fbs',
  'state.fbs',
  'control.fbs',
  'transport.fbs',
  'optical_flow.fbs',
  'mocap.fbs',
  'log.fbs',
  'sil.fbs',
  'all.fbs'
]);

/**
 * Resolve the absolute path to a shipped `.fbs` schema file.
 * @param {string} name Schema file name, e.g. `log.fbs`.
 * @returns {string} Absolute filesystem path.
 */
export function schemaPath(name) {
  return fileURLToPath(new URL(`fbs/${name}`, packageRoot));
}
