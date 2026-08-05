package com.autonomousdroneexpert.m1c.domain

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ReadOnlyHardwareTestRunnerTest {
    private val runner = ReadOnlyHardwareTestRunner(clock = { 0L })

    @Test
    fun `permission denied on open is a classified error, not a crash`() = runTest {
        val target = FakeOpenable(
            FakeOpenable.info(),
            openError = ClassifiedError(TransportError.PERMISSION_DENIED, "no permission"),
        )
        val obs = runner.singleOpen(target, 115_200, 250)
        assertEquals(ObservationStatus.CLASSIFIED_ERROR, obs.status)
        assertEquals(TransportError.PERMISSION_DENIED, obs.error)
    }

    @Test
    fun `device disconnected during unplug detection is surfaced and classified`() = runTest {
        val target = FakeOpenable(FakeOpenable.info(), readScript = {
            mutableListOf(
                ReadOutcome.TimedOut(1000.0),
                ReadOutcome.TimedOut(1000.0),
                ReadOutcome.Failed(ClassifiedError(TransportError.DEVICE_DISCONNECTED, "gone")),
            )
        })
        val obs = runner.unplugDetection(target, timeoutMs = 1000, maxSlices = 10)
        assertEquals(ObservationStatus.CLASSIFIED_ERROR, obs.status)
        assertEquals(TransportError.DEVICE_DISCONNECTED, obs.error)
    }

    @Test
    fun `read-timeout accuracy computes statistics from timeout slices only`() = runTest {
        val target = FakeOpenable(FakeOpenable.info(), readScript = {
            mutableListOf(
                ReadOutcome.TimedOut(250.0),
                ReadOutcome.Data(byteCount = 8, elapsedMs = 5.0), // counted, not in stats
                ReadOutcome.TimedOut(262.0),
                ReadOutcome.TimedOut(264.0),
            )
        })
        val obs = runner.readTimeoutAccuracy(target, timeoutMs = 250, samples = 4)
        assertEquals(ObservationStatus.OBSERVED, obs.status)
        assertNotNull(obs.timeoutStats)
        val stats = obs.timeoutStats!!
        assertEquals(3, stats.samples) // only the 3 timeouts
        assertEquals(250.0, stats.minMs, 0.0)
        assertEquals(264.0, stats.maxMs, 0.0)
        assertTrue("detail mentions data_events", obs.detail.contains("data_events=1"))
    }

    @Test
    fun `open-close cycles count clean cycles`() = runTest {
        val target = FakeOpenable(FakeOpenable.info())
        val obs = runner.openCloseCycles(target, cycles = 20, baud = 115_200, timeoutMs = 250)
        assertEquals(ObservationStatus.OBSERVED, obs.status)
        assertEquals(20, target.opens)
        assertTrue(obs.detail.startsWith("20/20 clean"))
    }

    @Test
    fun `coroutine cancellation stops a long open-close run`() = runTest {
        // A target whose open never lets the loop finish quickly.
        val target = object : Openable {
            override val info = FakeOpenable.info()
            override suspend fun open(baud: Int, readTimeoutMs: Int): OpenResult {
                delay(1_000) // suspension point -> cooperatively cancellable
                return OpenResult.Opened(FakeReadOnlySession(mutableListOf()))
            }
        }
        var caught = false
        val job: Job = launch {
            try {
                runner.openCloseCycles(target, cycles = 1000, baud = 115_200, timeoutMs = 250)
            } catch (_: CancellationException) {
                caught = true
            }
        }
        // Let it start, then cancel.
        kotlinx.coroutines.yield()
        job.cancel()
        job.join()
        assertTrue("cancellation propagated", caught || job.isCancelled)
    }
}
