package com.autonomousdroneexpert.m1c.domain

/**
 * TEST-ONLY simulation. This is NOT a real transport and never touches hardware. It
 * scripts read/open outcomes so the runner can be tested deterministically on the JVM.
 * It lives under src/test and is clearly marked as a simulation.
 */
class FakeReadOnlySession(
    private val script: MutableList<ReadOutcome>,
) : ReadOnlySession {
    var closed = false
        private set

    override suspend fun read(): ReadOutcome =
        if (script.isNotEmpty()) script.removeAt(0) else ReadOutcome.TimedOut(250.0)

    override fun close() {
        closed = true
    }
}

class FakeOpenable(
    override val info: UsbDeviceInfo,
    private val openError: ClassifiedError? = null,
    private val readScript: () -> MutableList<ReadOutcome> = { mutableListOf() },
) : Openable {
    var opens = 0
        private set

    override suspend fun open(baud: Int, readTimeoutMs: Int): OpenResult {
        opens++
        return openError?.let { OpenResult.Failed(it) }
            ?: OpenResult.Opened(FakeReadOnlySession(readScript()))
    }

    companion object {
        fun info(deviceId: Int = 1) = UsbDeviceInfo(
            deviceName = "/dev/fake$deviceId",
            vid = 0x0483,
            pid = 0x5740,
            manufacturer = "STMicroelectronics",
            product = "Virtual COM Port",
            serial = null,
            androidDeviceId = deviceId,
            permissionGranted = true,
            driverMatch = "interfaces=1 firstClass=2",
        )
    }
}
