import {
  WasmReadonlySerialDirective,
  WasmReadonlySerialDiscovery,
} from "/wasm/ade_web_readonly_serial_wasm_bridge.js";

const INITIAL_MSP_BAUD_RATE = 115200;
const DEFAULT_EXCHANGE_TIMEOUT_MS = 1500;
const MAX_CHUNKS_PER_EXCHANGE = 128;

function stableFailure(error, fallback = "Unknown") {
  switch (error?.name) {
    case "NotFoundError":
    case "AbortError":
      return "Cancelled";
    case "NotAllowedError":
    case "SecurityError":
      return "PermissionDenied";
    case "InvalidStateError":
      return "PortBusy";
    case "NetworkError":
      return "Disconnected";
    default:
      return fallback;
  }
}

function timeoutRead(reader, milliseconds) {
  let timer;
  return Promise.race([
    reader.read().finally(() => clearTimeout(timer)),
    new Promise((_, reject) => {
      timer = setTimeout(() => {
        const error = new Error("bounded serial response timeout");
        error.name = "TimeoutError";
        reject(error);
      }, milliseconds);
    }),
  ]);
}

/**
 * Narrow host for Rust-issued read-only discovery directives.
 *
 * The selected port is memory-only. This class never enumerates grants, reads USB metadata,
 * logs frames, constructs MSP, or accepts a caller-selected command.
 */
export class WebSerialReadonlyHost {
  #serial;
  #selectedPort = null;
  #opened = false;
  #reader = null;
  #readerCancelled = false;
  #writer = null;
  #timeoutMs;

  constructor({
    serial = globalThis.navigator?.serial,
    timeoutMs = DEFAULT_EXCHANGE_TIMEOUT_MS,
  } = {}) {
    this.#serial = serial;
    this.#timeoutMs = timeoutMs;
  }

  /** Must be called directly from the owner-controlled user gesture handler. */
  async selectPortFromUserGesture() {
    if (!this.#serial?.requestPort) return { ok: false, failure: "Unavailable" };
    try {
      this.#selectedPort = await this.#serial.requestPort();
      return { ok: true };
    } catch (error) {
      this.#selectedPort = null;
      return { ok: false, failure: stableFailure(error) };
    }
  }

  async #cleanup() {
    let failure = null;
    if (this.#reader) {
      if (!this.#readerCancelled) {
        try {
          await this.#reader.cancel();
        } catch (error) {
          failure ??= stableFailure(error);
        }
      }
      try {
        this.#reader.releaseLock();
      } catch (error) {
        failure ??= stableFailure(error);
      }
      this.#reader = null;
      this.#readerCancelled = false;
    }
    if (this.#writer) {
      try {
        this.#writer.releaseLock();
      } catch (error) {
        failure ??= stableFailure(error);
      }
      this.#writer = null;
    }
    if (this.#opened && this.#selectedPort) {
      try {
        await this.#selectedPort.close();
      } catch (error) {
        failure ??= stableFailure(error);
      }
    }
    this.#opened = false;
    this.#selectedPort = null;
    return failure;
  }

  async #exchange(discovery, directive) {
    try {
      this.#writer ??= this.#selectedPort?.writable?.getWriter();
      this.#reader ??= this.#selectedPort?.readable?.getReader();
      if (!this.#writer || !this.#reader) {
        return discovery.acceptExchangeFailure(directive.requestId, "Disconnected");
      }
      await this.#writer.write(directive.bytes);
      for (let chunks = 0; chunks < MAX_CHUNKS_PER_EXCHANGE; chunks += 1) {
        const { value, done } = await timeoutRead(this.#reader, this.#timeoutMs);
        if (done || !(value instanceof Uint8Array)) {
          return discovery.acceptExchangeFailure(directive.requestId, "Disconnected");
        }
        const next = discovery.acceptReadChunk(directive.requestId, value);
        if (next) return next;
      }
      return discovery.acceptExchangeFailure(directive.requestId, "Timeout");
    } catch (error) {
      if (error?.name === "TimeoutError") {
        try {
          await this.#reader?.cancel();
          this.#readerCancelled = true;
        } catch {
          // The Rust result still remains Timeout; cleanup runs at the close directive.
        }
        return discovery.acceptExchangeFailure(directive.requestId, "Timeout");
      }
      return discovery.acceptExchangeFailure(
        directive.requestId,
        stableFailure(error, "Disconnected"),
      );
    }
  }

  /** Create and run the exact generated Rust state machine after explicit selection. */
  async discover() {
    if (!this.#selectedPort) return { outcome: "failed", failure: "Unavailable" };
    const discovery = new WasmReadonlySerialDiscovery();
    try {
      let directive = discovery.begin();
      while (directive) {
        if (!(directive instanceof WasmReadonlySerialDirective)) {
          throw new TypeError("RUST_WEB_SERIAL_REFUSAL:UNTRUSTED_DIRECTIVE");
        }
        const current = directive;
        switch (current.kind) {
          case "open-selected-read-only-port":
            try {
              await this.#selectedPort.open({ baudRate: INITIAL_MSP_BAUD_RATE });
              this.#opened = true;
              directive = discovery.acceptOpenSuccess(current.requestId);
            } catch (error) {
              discovery.acceptOpenFailure(current.requestId, stableFailure(error, "Unknown"));
              directive = null;
            }
            break;
          case "exchange-identification-read":
            directive = await this.#exchange(discovery, current);
            break;
          case "close": {
            const closeFailure = await this.#cleanup();
            if (closeFailure) discovery.acceptCloseFailure(current.requestId, closeFailure);
            else discovery.acceptCloseSuccess(current.requestId);
            directive = null;
            break;
          }
          default:
            throw new Error("RUST_WEB_SERIAL_REFUSAL:UNKNOWN_DIRECTIVE_KIND");
        }
        current.free();
      }
      return {
        outcome: discovery.outcomeKind,
        failure: discovery.failureClass ?? undefined,
        scopeMismatchField: discovery.scopeMismatchField ?? undefined,
        apiVersion: discovery.apiVersion ?? undefined,
        fcVariant: discovery.fcVariant ?? undefined,
        fcVersion: discovery.fcVersion ?? undefined,
        targetName: discovery.targetName ?? undefined,
        hardwareObserved: discovery.hardwareObserved,
      };
    } finally {
      if (this.#selectedPort || this.#opened || this.#reader || this.#writer) {
        await this.#cleanup();
      }
      discovery.free();
    }
  }
}

export const WEB_SERIAL_READONLY_INITIAL_BAUD_RATE = INITIAL_MSP_BAUD_RATE;
