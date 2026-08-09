import {
  classifyStorageException,
  decideCompareAndSwap,
  formatStorageRevision,
  isExpectedJournalObjectStoreSchema,
  isStorageRevision,
  isValidStorageKey,
  STORAGE_RECORD_SCHEMA_VERSION,
  validateStoredJournalRecord,
} from "./journal-storage-contract.mjs";
import type {
  StorageFailure,
  StorageResult,
  StoredJournalRecord,
  StoredJournalValue,
} from "./journal-storage-contract.mjs";

export const JOURNAL_DATABASE_NAME = "autonomous-drone-expert-journal";
export const JOURNAL_DATABASE_VERSION = 1;
export const JOURNAL_OBJECT_STORE_NAME = "journals";

type OpenResult = StorageResult<IDBDatabase>;

function defaultIndexedDbFactory(): IDBFactory | undefined {
  try {
    return globalThis.indexedDB;
  } catch {
    return undefined;
  }
}

function failure<T>(value: StorageFailure): StorageResult<T> {
  return { ok: false, failure: value };
}

function success<T>(value: T): StorageResult<T> {
  return { ok: true, value };
}

export class IndexedDbJournalStore {
  readonly #factory: IDBFactory | undefined;
  readonly #connections = new Set<IDBDatabase>();

  constructor(factory: IDBFactory | undefined = defaultIndexedDbFactory()) {
    this.#factory = factory;
  }

  close(): void {
    for (const database of this.#connections) database.close();
    this.#connections.clear();
  }

  async load(key: string): Promise<StorageResult<StoredJournalValue | null>> {
    if (!isValidStorageKey(key)) return failure("Corrupt");
    const opened = await this.#openDatabase();
    if (!opened.ok) return opened;
    const database = opened.value;
    try {
      return await this.#loadTransaction(database, key);
    } finally {
      this.#release(database);
    }
  }

  async compareAndSwap(
    key: string,
    expectedRevision: bigint | null,
    bytes: Uint8Array,
  ): Promise<StorageResult<bigint>> {
    if (
      !isValidStorageKey(key) ||
      (expectedRevision !== null && !isStorageRevision(expectedRevision)) ||
      !(bytes instanceof Uint8Array) ||
      bytes.byteLength === 0
    ) {
      return failure("Corrupt");
    }
    const committedBytes = Uint8Array.from(bytes);
    const opened = await this.#openDatabase();
    if (!opened.ok) return opened;
    const database = opened.value;
    try {
      return await this.#compareAndSwapTransaction(
        database,
        key,
        expectedRevision,
        committedBytes,
      );
    } finally {
      this.#release(database);
    }
  }

  #release(database: IDBDatabase): void {
    this.#connections.delete(database);
    database.close();
  }

  #openDatabase(): Promise<OpenResult> {
    const factory = this.#factory;
    if (!factory) return Promise.resolve(failure("Unavailable"));

    return new Promise((resolve) => {
      let request: IDBOpenDBRequest;
      let settled = false;
      let incompatibleSchema = false;

      const finish = (result: OpenResult): void => {
        if (settled) return;
        settled = true;
        resolve(result);
      };

      try {
        request = factory.open(JOURNAL_DATABASE_NAME, JOURNAL_DATABASE_VERSION);
      } catch (error) {
        finish(failure(classifyStorageException(error)));
        return;
      }

      request.addEventListener("upgradeneeded", (event) => {
        const database = request.result;
        if (
          event.oldVersion !== 0 ||
          event.newVersion !== JOURNAL_DATABASE_VERSION ||
          database.objectStoreNames.length !== 0
        ) {
          incompatibleSchema = true;
          request.transaction?.abort();
          return;
        }
        database.createObjectStore(JOURNAL_OBJECT_STORE_NAME, { keyPath: "key" });
      });

      request.addEventListener("blocked", () => finish(failure("Unavailable")));
      request.addEventListener("error", () => {
        finish(
          failure(
            incompatibleSchema ? "Unavailable" : classifyStorageException(request.error),
          ),
        );
      });
      request.addEventListener("success", () => {
        const database = request.result;
        if (settled) {
          database.close();
          return;
        }
        if (
          database.version !== JOURNAL_DATABASE_VERSION ||
          database.objectStoreNames.length !== 1 ||
          !database.objectStoreNames.contains(JOURNAL_OBJECT_STORE_NAME)
        ) {
          database.close();
          finish(failure("Unavailable"));
          return;
        }
        try {
          const objectStore = database
            .transaction(JOURNAL_OBJECT_STORE_NAME, "readonly")
            .objectStore(JOURNAL_OBJECT_STORE_NAME);
          if (
            !isExpectedJournalObjectStoreSchema(
              objectStore.keyPath,
              objectStore.autoIncrement,
              objectStore.indexNames.length,
            )
          ) {
            database.close();
            finish(failure("Unavailable"));
            return;
          }
        } catch {
          database.close();
          finish(failure("Unavailable"));
          return;
        }
        database.addEventListener(
          "versionchange",
          () => {
            this.#connections.delete(database);
            database.close();
          },
          { once: true },
        );
        this.#connections.add(database);
        finish(success(database));
      });
    });
  }

  #loadTransaction(
    database: IDBDatabase,
    key: string,
  ): Promise<StorageResult<StoredJournalValue | null>> {
    return new Promise((resolve) => {
      let transaction: IDBTransaction;
      try {
        transaction = database.transaction(JOURNAL_OBJECT_STORE_NAME, "readonly");
      } catch (error) {
        resolve(failure(classifyStorageException(error)));
        return;
      }

      let result: StorageResult<StoredJournalValue | null> | undefined;
      let semanticFailure: StorageFailure | undefined;
      let browserError: unknown;
      let settled = false;
      const finish = (value: StorageResult<StoredJournalValue | null>): void => {
        if (settled) return;
        settled = true;
        resolve(value);
      };

      transaction.addEventListener("complete", () => {
        finish(result ?? failure("Unknown"));
      });
      transaction.addEventListener("error", () => {
        browserError = transaction.error;
      });
      transaction.addEventListener("abort", () => {
        finish(
          failure(
            semanticFailure ?? classifyStorageException(browserError ?? transaction.error),
          ),
        );
      });

      const request = transaction.objectStore(JOURNAL_OBJECT_STORE_NAME).get(key);
      request.addEventListener("error", () => {
        browserError = request.error;
      });
      request.addEventListener("success", () => {
        if (request.result === undefined) {
          result = success(null);
          return;
        }
        const validated = validateStoredJournalRecord(request.result, key);
        if (!validated.ok) {
          semanticFailure = validated.failure;
          transaction.abort();
          return;
        }
        result = success(validated.value);
      });
    });
  }

  #compareAndSwapTransaction(
    database: IDBDatabase,
    key: string,
    expectedRevision: bigint | null,
    bytes: Uint8Array,
  ): Promise<StorageResult<bigint>> {
    return new Promise((resolve) => {
      let transaction: IDBTransaction;
      try {
        transaction = database.transaction(JOURNAL_OBJECT_STORE_NAME, "readwrite");
      } catch (error) {
        resolve(failure(classifyStorageException(error)));
        return;
      }

      let semanticFailure: StorageFailure | undefined;
      let browserError: unknown;
      let commitRevision: bigint | undefined;
      let settled = false;
      const finish = (value: StorageResult<bigint>): void => {
        if (settled) return;
        settled = true;
        resolve(value);
      };

      transaction.addEventListener("complete", () => {
        finish(commitRevision === undefined ? failure("Unknown") : success(commitRevision));
      });
      transaction.addEventListener("error", () => {
        browserError = transaction.error;
      });
      transaction.addEventListener("abort", () => {
        finish(
          failure(
            semanticFailure ?? classifyStorageException(browserError ?? transaction.error),
          ),
        );
      });

      const objectStore = transaction.objectStore(JOURNAL_OBJECT_STORE_NAME);
      const readRequest = objectStore.get(key);
      readRequest.addEventListener("error", () => {
        browserError = readRequest.error;
      });
      readRequest.addEventListener("success", () => {
        const decision = decideCompareAndSwap(readRequest.result, expectedRevision, key);
        if (decision.kind === "conflict") {
          semanticFailure = "Conflict";
          transaction.abort();
          return;
        }
        if (decision.kind === "failure") {
          semanticFailure = decision.failure;
          transaction.abort();
          return;
        }

        const revision = formatStorageRevision(decision.revision);
        if (revision === null) {
          semanticFailure = "Unknown";
          transaction.abort();
          return;
        }
        const record: StoredJournalRecord = {
          key,
          schemaVersion: STORAGE_RECORD_SCHEMA_VERSION,
          revision,
          bytes: Uint8Array.from(bytes).buffer,
        };
        commitRevision = decision.revision;
        const putRequest = objectStore.put(record);
        putRequest.addEventListener("error", () => {
          browserError = putRequest.error;
        });
      });
    });
  }
}
