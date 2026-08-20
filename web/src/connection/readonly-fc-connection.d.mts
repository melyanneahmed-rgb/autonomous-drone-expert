import type {
  DiagnosticOrigin,
  DiagnosticTraceEvent,
} from "../diagnostics/readonly-trace.mjs";

export type ReadonlyFcFailure =
  | "Unavailable"
  | "Cancelled"
  | "PermissionDenied"
  | "PortBusy"
  | "Disconnected"
  | "Timeout"
  | "MalformedResponse"
  | "ProtocolIdentityFailure"
  | "CloseFailure"
  | "HardwareEvidenceBoundary"
  | "Unknown";

export interface PortSelectionResult {
  ok: boolean;
  failure?: ReadonlyFcFailure;
  failureOrigin?: DiagnosticOrigin;
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
  outcome:
    | "in-scope"
    | "scope-mismatch"
    | "read-only-complete"
    | "read-profile-unsupported"
    | "api-unsupported"
    | "failed"
    | "pending";
  apiVersion?: string;
  fcVariant?: string;
  fcVersion?: string;
  targetName?: string;
  scopeMismatchField?: string;
  failure?: ReadonlyFcFailure | string;
  failureOrigin?: DiagnosticOrigin;
  failureStage?: IdentityFailureStage;
  failureReason?: IdentityFailureReason;
}

export interface ReadonlyFcConnection {
  selectPortFromUserGesture(): Promise<PortSelectionResult>;
  discover(): Promise<PrivacyBoundedIdentityResult>;
  recordUiBoundaryFailure(): void;
  diagnosticTrace(): readonly DiagnosticTraceEvent[];
  safeDiagnosticTraceText(): string;
  clearDiagnosticTrace(): void;
}

export declare function prepareReadonlyFcConnection(): Promise<ReadonlyFcConnection>;
