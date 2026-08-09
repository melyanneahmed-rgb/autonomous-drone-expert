import type { IndexedDbJournalStore } from "./indexeddb-journal-store";

export type StorageDirective = {
  readonly requestId: string;
  readonly kind: "load" | "compare-and-swap";
  readonly key: string;
  readonly expectedRevision: string | undefined;
  readonly bytes: Uint8Array;
  free?(): void;
};

export interface RustJournalBridge {
  beginLoad(): StorageDirective;
  acceptLoadMissing(requestId: string): string;
  acceptLoadFound(requestId: string, revision: string, bytes: Uint8Array): string;
  acceptLoadFailure(requestId: string, failure: string): string;
  acceptCommitSuccess(requestId: string, revision: string): string;
  acceptCommitFailure(requestId: string, failure: string): string;
  takeRepairEffect(): StorageDirective;
}

export class StorageWasmHostProtocolError extends Error {}

export function parseCanonicalU64(value: unknown, label?: string): bigint;
export function formatCanonicalU64(value: unknown, label?: string): string;

export class WasmJournalHost {
  constructor(bridge: RustJournalBridge, store: IndexedDbJournalStore);
  execute(directive: StorageDirective): Promise<string>;
  load(): Promise<"loaded" | "repair-committed">;
}
