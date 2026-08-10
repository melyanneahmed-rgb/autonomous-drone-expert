export type WebSerialFailure =
  | "Unavailable"
  | "Cancelled"
  | "PermissionDenied"
  | "PortBusy"
  | "Disconnected"
  | "Timeout"
  | "Unknown";

export interface PortSelectionResult {
  ok: boolean;
  failure?: WebSerialFailure;
}

export interface ReadonlyDiscoveryResult {
  outcome: "in-scope" | "scope-mismatch" | "failed" | "pending";
  failure?: string;
  scopeMismatchField?: string;
  apiVersion?: string;
  fcVariant?: string;
  fcVersion?: string;
  targetName?: string;
  hardwareObserved: false;
}

export declare class WebSerialReadonlyHost {
  constructor(options?: { serial?: object; timeoutMs?: number });
  selectPortFromUserGesture(): Promise<PortSelectionResult>;
  discover(): Promise<ReadonlyDiscoveryResult>;
}

export declare const WEB_SERIAL_READONLY_INITIAL_BAUD_RATE: 115200;
