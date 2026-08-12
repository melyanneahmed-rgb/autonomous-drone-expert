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

export type IdentityFailureStage =
  | "API_VERSION"
  | "FC_VARIANT"
  | "FC_VERSION"
  | "BOARD_INFO";

export type IdentityFailureReason =
  | "PayloadTooLong"
  | "FrameTooLarge"
  | "Truncated"
  | "TrailingBytes"
  | "BadPreamble"
  | "BadDirection"
  | "BadChecksum"
  | "WrongCommand"
  | "WrongDirection"
  | "ErrorReply"
  | "ReplyMisclassified"
  | "WrongLength"
  | "FieldOverrun"
  | "TrailingPayload"
  | "InvalidUtf8"
  | "OtherProtocolIdentityFailure";

export interface PrivacyBoundedIdentityResult {
  outcome: "in-scope" | "scope-mismatch" | "failed" | "pending";
  apiVersion?: string;
  fcVariant?: string;
  fcVersion?: string;
  targetName?: string;
  scopeMismatchField?: string;
  failure?: ReadonlyFcFailure | string;
  failureStage?: IdentityFailureStage;
  failureReason?: IdentityFailureReason;
}

export interface ReadonlyFcConnection {
  selectPortFromUserGesture(): Promise<PortSelectionResult>;
  discover(): Promise<PrivacyBoundedIdentityResult>;
}

export declare function prepareReadonlyFcConnection(): Promise<ReadonlyFcConnection>;
