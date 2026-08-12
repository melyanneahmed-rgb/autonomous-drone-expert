import initReadonlySerialWasm from "/wasm/ade_web_readonly_serial_wasm_bridge.js";

import { WebSerialReadonlyHost } from "../transport/webserial-readonly-host.mjs";

const WASM_ASSET_PATH = "/wasm/ade_web_readonly_serial_wasm_bridge_bg.wasm";

function privacyBoundedResult(result) {
  if (result.hardwareObserved !== false) {
    return { outcome: "failed", failure: "HardwareEvidenceBoundary" };
  }
  return {
    outcome: result.outcome,
    apiVersion: result.apiVersion,
    fcVariant: result.fcVariant,
    fcVersion: result.fcVersion,
    targetName: result.targetName,
    scopeMismatchField: result.scopeMismatchField,
    failure: result.failure,
    failureStage: result.failureStage,
    failureReason: result.failureReason,
  };
}

class PreparedReadonlyFcConnection {
  #host = new WebSerialReadonlyHost();

  async selectPortFromUserGesture() {
    const selection = await this.#host.selectPortFromUserGesture();
    return selection;
  }

  async discover() {
    return privacyBoundedResult(await this.#host.discover());
  }
}

/** Prepare the audited Rust runtime before the product enables either connection button. */
export async function prepareReadonlyFcConnection() {
  await initReadonlySerialWasm({
    module_or_path: new URL(WASM_ASSET_PATH, globalThis.location.href),
  });
  return Object.freeze(new PreparedReadonlyFcConnection());
}
