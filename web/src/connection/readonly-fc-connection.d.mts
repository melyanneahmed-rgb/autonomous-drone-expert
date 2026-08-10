export type ReadonlyFcFailure =
  | "Unavailable"
  | "Cancelled"
  | "PermissionDenied"
  | "PortBusy"
  | "Disconnected"
  | "Timeout"
  | "MalformedResponse"
  | "ProtocolFailure"
  | "HardwareEvidenceBoundary"
  | "Unknown";

export interface PortSelectionResult {
  ok: boolean;
  failure?: ReadonlyFcFailure;
}

export interface PrivacyBoundedIdentityResult {
  outcome: "in-scope" | "scope-mismatch" | "failed" | "pending";
  apiVersion?: string;
  fcVariant?: string;
  fcVersion?: string;
  targetName?: string;
  scopeMismatchField?: string;
  failure?: ReadonlyFcFailure | string;
}

export interface ReadonlyFcConnection {
  selectPortFromUserGesture(): Promise<PortSelectionResult>;
  discover(): Promise<PrivacyBoundedIdentityResult>;
}

export declare function prepareReadonlyFcConnection(): Promise<ReadonlyFcConnection>;
