export type StorageFailure =
  | "Conflict"
  | "QuotaExceeded"
  | "Unavailable"
  | "Corrupt"
  | "Cancelled"
  | "Unknown";

export type StorageResult<T> =
  | { ok: true; value: T }
  | { ok: false; failure: StorageFailure };

export type StoredJournalValue = {
  revision: bigint;
  bytes: Uint8Array;
};

export type StoredJournalRecord = {
  key: string;
  schemaVersion: number;
  revision: string;
  bytes: ArrayBuffer;
};

export type CompareAndSwapDecision =
  | { kind: "commit"; revision: bigint }
  | { kind: "conflict" }
  | { kind: "failure"; failure: StorageFailure };

export const STORAGE_RECORD_SCHEMA_VERSION: 1;
export const MAX_STORAGE_REVISION: 18446744073709551615n;

export function isValidStorageKey(value: unknown): value is string;
export function isStorageRevision(value: unknown): value is bigint;
export function parseStorageRevision(value: unknown): bigint | null;
export function formatStorageRevision(value: unknown): string | null;
export function isExpectedJournalObjectStoreSchema(
  keyPath: unknown,
  autoIncrement: unknown,
  indexCount: unknown,
): boolean;
export function classifyStorageException(error: unknown): StorageFailure;
export function validateStoredJournalRecord(
  record: unknown,
  expectedKey: unknown,
): StorageResult<StoredJournalValue>;
export function decideCompareAndSwap(
  record: unknown | undefined,
  expectedRevision: unknown | null,
  expectedKey: unknown,
): CompareAndSwapDecision;
