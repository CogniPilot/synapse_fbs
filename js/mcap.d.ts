import type { IWritable, TypedMcapRecord } from '@mcap/core';
import type { TopicInfo } from './topic_catalog.js';

export * as container from '@mcap/core';

export const TimeBasis: Readonly<{
  MONOTONIC_BOOT: string;
  UNIX_EPOCH: string;
  CORRELATED: string;
}>;

export interface TopicChannel {
  id: number;
  topic: TopicInfo;
  nextSequence: number;
}

export type SchemaLoader = (topic: TopicInfo) => Promise<Uint8Array>;

export interface WriterOptions {
  writable: IWritable;
  library: string;
  sessionId: string;
  source: string;
  timeBasis?: string;
  schemaLoader?: SchemaLoader;
}

export function loadSchema(topic: string | TopicInfo): Promise<Uint8Array>;
export function wrapFixedPayload(payload: Uint8Array | ArrayBuffer): Uint8Array;

export class Writer {
  constructor(options: WriterOptions);
  start(): Promise<void>;
  addTopic(topic: string | TopicInfo, channelTopic: string): Promise<TopicChannel>;
  write(
    channel: TopicChannel,
    logTimeNs: bigint | number,
    publishTimeNs: bigint | number,
    data: Uint8Array | ArrayBuffer,
  ): Promise<void>;
  writeFixed(
    channel: TopicChannel,
    logTimeNs: bigint | number,
    publishTimeNs: bigint | number,
    payload: Uint8Array | ArrayBuffer,
  ): Promise<void>;
  finish(): Promise<void>;
}

export class Reader {
  constructor(options?: ConstructorParameters<typeof import('@mcap/core').McapStreamReader>[0]);
  append(data: Uint8Array): void;
  nextRecord(): TypedMcapRecord | undefined;
  done(): boolean;
  assertValid(): void;
}
