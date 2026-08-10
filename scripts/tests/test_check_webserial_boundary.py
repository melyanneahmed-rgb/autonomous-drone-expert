"""Adversarial tests for the Web Serial read-only authority gate."""

from __future__ import annotations

import copy
import sys
import unittest
from pathlib import Path, PurePosixPath

SCRIPTS = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(SCRIPTS))

import check_webserial_boundary  # noqa: E402


class WebSerialAuthorityPolicyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.sources = check_webserial_boundary.product_sources()
        self.assertEqual(check_webserial_boundary.source_authority_errors(self.sources), [])

    def rejected(self, relative: str, injected: str) -> None:
        sources = copy.deepcopy(self.sources)
        path = PurePosixPath(relative)
        sources[path] = sources.get(path, "") + f"\n{injected}\n"
        self.assertTrue(check_webserial_boundary.source_authority_errors(sources))

    def test_serial_authority_in_react_or_service_worker_is_rejected(self) -> None:
        self.rejected("web/src/App.tsx", "globalThis.navigator.serial.requestPort()")
        self.rejected("web/public/sw.js", "navigator.serial.requestPort()")

    def test_request_port_or_writer_outside_adapter_is_rejected(self) -> None:
        self.rejected("web/src/escape.mjs", "navigator.serial.requestPort()")
        self.rejected("web/src/escape.mjs", "thing.#writer.write(bytes)")

    def test_get_ports_and_raw_api_names_are_rejected(self) -> None:
        adapter = str(check_webserial_boundary.ADAPTER)
        self.rejected(adapter, "await this.#serial.getPorts()")
        for name in ("sendRaw", "writeRaw", "sendMsp", "writeCommand", "executeArbitraryBytes"):
            self.rejected(adapter, f"function {name}(value) {{ return value; }}")

    def test_javascript_command_and_write_authority_types_are_rejected(self) -> None:
        adapter = str(check_webserial_boundary.ADAPTER)
        for marker in ("CommandId", "WriteApproval", "TransportEffect", "OutboundPacket"):
            self.rejected(adapter, f"const leaked = {marker};")

    def test_javascript_msp_semantics_logging_and_persistence_are_rejected(self) -> None:
        adapter = str(check_webserial_boundary.ADAPTER)
        for marker in (
            "const MSP_REBOOT = 68;",
            "buildMsp(frame);",
            "console.log(bytes);",
            "localStorage.setItem('port', value);",
            "indexedDB.open('serial');",
        ):
            self.rejected(adapter, marker)

    def test_webusb_and_webhid_are_rejected(self) -> None:
        self.rejected("web/src/escape.mjs", "navigator.usb.requestDevice()")
        self.rejected("web/src/escape.mjs", "navigator.hid.requestDevice()")

    def test_missing_adapter_or_declaration_fails_closed(self) -> None:
        for required in (
            check_webserial_boundary.ADAPTER,
            check_webserial_boundary.DECLARATION,
            check_webserial_boundary.FACADE,
            check_webserial_boundary.FACADE_DECLARATION,
            check_webserial_boundary.APP,
        ):
            sources = copy.deepcopy(self.sources)
            del sources[required]
            self.assertTrue(check_webserial_boundary.source_authority_errors(sources))

    def test_substitutable_rust_trust_inputs_are_rejected(self) -> None:
        adapter = str(check_webserial_boundary.ADAPTER)
        for marker in (
            "const rustDirectiveType = FakeDirective;",
            "function setRustBindings(value) { return value; }",
            "const rustBindings = caller.bindings;",
            "const bindings = caller.bindings;",
            "const bindingFactory = caller.factory;",
            "const bindingsFactory = caller.factory;",
            "const discoveryFactory = caller.factory;",
            "const directiveFactory = caller.factory;",
            "const validator = caller.validator;",
            "const trustCallback = caller.callback;",
            "const directiveType = caller.type;",
            "const discoveryType = caller.type;",
            "const directiveConstructor = caller.type;",
            "const discoveryConstructor = caller.type;",
            "async discover(discovery) {}",
        ):
            self.rejected(adapter, marker)

    def test_generated_binding_import_is_exact_and_non_replaceable(self) -> None:
        sources = copy.deepcopy(self.sources)
        adapter = check_webserial_boundary.ADAPTER
        sources[adapter] = sources[adapter].replace(
            'from "/wasm/ade_web_readonly_serial_wasm_bridge.js"',
            'from "/wasm/caller-selected-bindings.js"',
        )
        self.assertTrue(check_webserial_boundary.source_authority_errors(sources))

        sources = copy.deepcopy(self.sources)
        sources[adapter] = sources[adapter].replace(
            "WasmReadonlySerialDirective,",
            "WasmReadonlySerialDirective as CallerDirective,",
        )
        self.assertTrue(check_webserial_boundary.source_authority_errors(sources))

        sources = copy.deepcopy(self.sources)
        sources[adapter] = sources[adapter].replace(
            "const discovery = new WasmReadonlySerialDiscovery();",
            "const discovery = this.discoveryFactory();",
        )
        self.assertTrue(check_webserial_boundary.source_authority_errors(sources))

        sources = copy.deepcopy(self.sources)
        sources[adapter] = sources[adapter].replace(
            "directive instanceof WasmReadonlySerialDirective",
            "directive instanceof this.directiveType",
        )
        self.assertTrue(check_webserial_boundary.source_authority_errors(sources))

    def test_public_declaration_cannot_accept_discovery_or_trust_types(self) -> None:
        sources = copy.deepcopy(self.sources)
        declaration = check_webserial_boundary.DECLARATION
        sources[declaration] = sources[declaration].replace(
            "discover(): Promise<ReadonlyDiscoveryResult>",
            "discover(discovery: object): Promise<ReadonlyDiscoveryResult>",
        )
        self.assertTrue(check_webserial_boundary.source_authority_errors(sources))

        sources = copy.deepcopy(self.sources)
        sources[declaration] = sources[declaration].replace(
            "timeoutMs?: number",
            "timeoutMs?: number; bindings?: object",
        )
        self.assertTrue(check_webserial_boundary.source_authority_errors(sources))

        for marker in (
            "rustDirectiveType: Function",
            "bindingsFactory: Function",
            "validator: Function",
        ):
            self.rejected(str(declaration), marker)

    def test_react_cannot_gain_command_payload_permission_or_persistence(self) -> None:
        for marker in (
            "navigator.serial.requestPort()",
            "getPorts()",
            "getInfo()",
            "const commandId = 1;",
            "const payload = bytes;",
            "WriteApproval",
            "TransportEffect",
            "localStorage.setItem('device', value)",
            "const hardwareObserved = true;",
        ):
            self.rejected(str(check_webserial_boundary.APP), marker)

    def test_facade_cannot_replace_host_or_gain_low_level_authority(self) -> None:
        for marker in (
            "navigator.serial.requestPort()",
            "getPorts()",
            "getInfo()",
            "const commandId = 1;",
            "const payload = bytes;",
            "WriteApproval",
            "TransportEffect",
            "localStorage.setItem('device', value)",
        ):
            self.rejected(str(check_webserial_boundary.FACADE), marker)


if __name__ == "__main__":
    unittest.main()
