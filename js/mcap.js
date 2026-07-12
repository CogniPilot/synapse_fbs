/** First-class support for the frozen synapse/1 MCAP profile.
 *
 * Container encoding and decoding are delegated to @mcap/core. This module
 * only applies the Synapse profile, catalog, embedded BFBS, and fixed-struct
 * wrapper.
 */
import { McapStreamReader, McapWriter } from '@mcap/core';
import {
  mcapMessageEncoding,
  mcapMetadataName,
  mcapProfile,
  mcapSchemaEncoding,
  mcapSchemaSetHashKey,
  mcapSessionIdKey,
  mcapSourceKey,
  mcapTimeBasisCorrelated,
  mcapTimeBasisKey,
  mcapTimeBasisMonotonicBoot,
  mcapTimeBasisUnixEpoch,
  mcapTopicIdKey,
  schemaSetHash,
  topicByName,
} from './topic_catalog.js';

export * as container from '@mcap/core';

export const TimeBasis = Object.freeze({
  MONOTONIC_BOOT: mcapTimeBasisMonotonicBoot,
  UNIX_EPOCH: mcapTimeBasisUnixEpoch,
  CORRELATED: mcapTimeBasisCorrelated,
});

function validSessionId(value) {
  return /^[0-9a-f]{32}$/.test(value);
}

function resolveTopic(value) {
  if (typeof value !== 'string') return value;
  const topic = topicByName(value);
  if (!topic) throw new Error(`unknown Synapse topic: ${value}`);
  return topic;
}

/** Load a package BFBS in Node or a browser/bundler deployment. */
export async function loadSchema(topic) {
  const info = resolveTopic(topic);
  const url = new URL(info.mcapSchemaFile, import.meta.url);
  if (url.protocol === 'file:') {
    const { readFile } = await import('node:fs/promises');
    return new Uint8Array(await readFile(url));
  }
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`failed to load ${url}: ${response.status} ${response.statusText}`);
  }
  return new Uint8Array(await response.arrayBuffer());
}

/** Wrap fixed struct bytes in their existing one-field FlatBuffer root. */
export function wrapFixedPayload(payload) {
  const data = payload instanceof Uint8Array ? payload : new Uint8Array(payload);
  const objectSize = data.byteLength + 4;
  if (objectSize > 0xffff || data.byteLength > 0xffffffff - 14) {
    throw new Error('fixed payload is too large to wrap');
  }
  const output = new Uint8Array(data.byteLength + 14);
  const view = new DataView(output.buffer);
  view.setUint32(0, 4, true);
  view.setInt32(4, -objectSize, true);
  output.set(data, 8);
  view.setUint16(8 + data.byteLength, 6, true);
  view.setUint16(10 + data.byteLength, objectSize, true);
  view.setUint16(12 + data.byteLength, 4, true);
  return output;
}

/** Uncompressed, unchunked, index-less synapse/1 writer. */
export class Writer {
  constructor({
    writable,
    library,
    sessionId,
    source,
    timeBasis = TimeBasis.MONOTONIC_BOOT,
    schemaLoader = loadSchema,
  }) {
    if (!library) throw new Error('MCAP library identifier is empty');
    if (!source) throw new Error('Synapse MCAP source is empty');
    if (!validSessionId(sessionId)) {
      throw new Error('Synapse MCAP session id must be 32 lowercase hexadecimal characters');
    }
    if (!Object.values(TimeBasis).includes(timeBasis)) {
      throw new Error(`unsupported Synapse MCAP time basis: ${timeBasis}`);
    }
    this._inner = new McapWriter({
      writable,
      useStatistics: false,
      useSummaryOffsets: false,
      useChunks: false,
      repeatSchemas: false,
      repeatChannels: false,
      useAttachmentIndex: false,
      useMetadataIndex: false,
      useMessageIndex: false,
      useChunkIndex: false,
    });
    this._library = library;
    this._metadata = new Map([
      [mcapSchemaSetHashKey, schemaSetHash],
      [mcapSessionIdKey, sessionId],
      [mcapSourceKey, source],
      [mcapTimeBasisKey, timeBasis],
    ]);
    this._schemaLoader = schemaLoader;
    this._schemaIds = new Map();
    this._started = false;
  }

  async start() {
    if (this._started) throw new Error('Synapse MCAP writer is already started');
    await this._inner.start({ profile: mcapProfile, library: this._library });
    await this._inner.addMetadata({ name: mcapMetadataName, metadata: this._metadata });
    this._started = true;
  }

  async addTopic(topic, channelTopic) {
    if (!this._started) throw new Error('call start() before addTopic()');
    const info = resolveTopic(topic);
    let schemaId = this._schemaIds.get(info.id);
    if (schemaId === undefined) {
      schemaId = await this._inner.registerSchema({
        name: info.mcapSchemaName,
        encoding: mcapSchemaEncoding,
        data: await this._schemaLoader(info),
      });
      this._schemaIds.set(info.id, schemaId);
    }
    const id = await this._inner.registerChannel({
      schemaId,
      topic: channelTopic,
      messageEncoding: mcapMessageEncoding,
      metadata: new Map([[mcapTopicIdKey, String(info.id)]]),
    });
    return { id, topic: info, nextSequence: 0 };
  }

  async write(channel, logTimeNs, publishTimeNs, data) {
    const bytes = data instanceof Uint8Array ? data : new Uint8Array(data);
    await this._inner.addMessage({
      channelId: channel.id,
      sequence: channel.nextSequence,
      logTime: BigInt(logTimeNs),
      publishTime: BigInt(publishTimeNs),
      data: bytes,
    });
    channel.nextSequence = (channel.nextSequence + 1) >>> 0;
  }

  async writeFixed(channel, logTimeNs, publishTimeNs, payload) {
    const bytes = payload instanceof Uint8Array ? payload : new Uint8Array(payload);
    if (!channel.topic.fixedLayout) {
      throw new Error(`${channel.topic.name} is not fixed-layout`);
    }
    if (bytes.byteLength !== channel.topic.payloadSize) {
      throw new Error(
        `fixed payload is ${bytes.byteLength} bytes, expected ${channel.topic.payloadSize}`,
      );
    }
    await this.write(channel, logTimeNs, publishTimeNs, wrapFixedPayload(bytes));
  }

  async finish() {
    if (!this._started) throw new Error('call start() before finish()');
    await this._inner.end();
  }
}

/** Streaming reader that validates the frozen Synapse profile as records arrive. */
export class Reader {
  constructor(options) {
    this._inner = new McapStreamReader(options);
    this._headerSeen = false;
    this._metadataSeen = false;
  }

  append(data) {
    this._inner.append(data);
  }

  nextRecord() {
    const record = this._inner.nextRecord();
    if (!record) return undefined;
    if (record.type === 'Header') {
      if (record.profile !== mcapProfile) {
        throw new Error(`unsupported MCAP profile: ${record.profile}`);
      }
      this._headerSeen = true;
    } else if (record.type === 'Metadata' && record.name === mcapMetadataName) {
      if (!/^[0-9a-f]{32}$/.test(record.metadata.get(mcapSchemaSetHashKey) ?? '')) {
        throw new Error('invalid Synapse MCAP schema-set hash');
      }
      if (!validSessionId(record.metadata.get(mcapSessionIdKey) ?? '')) {
        throw new Error('invalid Synapse MCAP session id');
      }
      if (!(record.metadata.get(mcapSourceKey) ?? '')) {
        throw new Error('missing Synapse MCAP source');
      }
      if (!Object.values(TimeBasis).includes(record.metadata.get(mcapTimeBasisKey))) {
        throw new Error('invalid Synapse MCAP time basis');
      }
      this._metadataSeen = true;
    } else if (record.type === 'Message' && (!this._headerSeen || !this._metadataSeen)) {
      throw new Error('message precedes required Synapse MCAP header or metadata');
    }
    return record;
  }

  done() {
    return this._inner.done();
  }

  assertValid() {
    if (!this.done()) throw new Error('MCAP input is incomplete');
    if (!this._headerSeen) throw new Error('missing Synapse MCAP header');
    if (!this._metadataSeen) throw new Error('missing required Synapse MCAP metadata');
  }
}
