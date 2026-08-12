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

export interface ReadonlyDiscoveryResult {
  outcome: "in-scope" | "scope-mismatch" | "failed" | "pending";
  failure?: string;
  failureStage?: IdentityFailureStage;
  failureReason?: IdentityFailureReason;
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
