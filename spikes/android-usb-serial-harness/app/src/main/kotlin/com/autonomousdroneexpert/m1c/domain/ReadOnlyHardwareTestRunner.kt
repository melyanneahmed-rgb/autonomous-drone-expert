package com.autonomousdroneexpert.m1c.domain

import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive

/**
 * Runs the read-only hardware stages against any [Openable] (real Android USB device in
 * the app; a fake in tests). Android-independent so it is fully JVM unit-testable.
 *
 * Cancellation: these are suspend functions that check coroutine cancellation between
 * reads via [ensureActive]. **Coroutine cancellation is NOT proof that the underlying
 * driver I/O was cancelled** -- an in-flight blocking read may still be running in the
 * platform layer until its own timeout. This distinction is deliberate and documented.
 *
 * @param clock injected elapsed-millis source so tests are deterministic (no wall clock).
 */
class ReadOnlyHardwareTestRunner(private val clock: () -> Long) {

    suspend fun singleOpen(target: Openable, baud: Int, timeoutMs: Int): HardwareObservation {
        val at = clock()
        return when (val r = target.open(baud, timeoutMs)) {
            is OpenResult.Opened -> {
                r.session.close()
                HardwareObservation(
                    stage = TestStage.SINGLE_OPEN,
                    status = ObservationStatus.OBSERVED,
                    detail = "opened and closed once; observe LED/COM/DFU/behaviour on the device",
                    atElapsedMillis = at,
                )
            }
            is OpenResult.Failed -> errorObservation(TestStage.SINGLE_OPEN, r.error, at)
        }
    }

    suspend fun openCloseCycles(target: Openable, cycles: Int, baud: Int, timeoutMs: Int): HardwareObservation {
        val at = clock()
        var clean = 0
        var firstError: ClassifiedError? = null
        repeat(cycles) {
            currentCoroutineContext().ensureActive()
            when (val r = target.open(baud, timeoutMs)) {
                is OpenResult.Opened -> { r.session.close(); clean++ }
                is OpenResult.Failed -> if (firstError == null) firstError = r.error
            }
        }
        return HardwareObservation(
            stage = TestStage.OPEN_CLOSE_CYCLES,
            status = if (firstError == null) ObservationStatus.OBSERVED else ObservationStatus.CLASSIFIED_ERROR,
            detail = "$clean/$cycles clean cycles" + (firstError?.let { "; first error ${it.error}" } ?: ""),
            error = firstError?.error,
            atElapsedMillis = at,
        )
    }

    suspend fun readTimeoutAccuracy(target: Openable, timeoutMs: Int, samples: Int): HardwareObservation {
        val at = clock()
        return when (val r = target.open(115_200, timeoutMs)) {
            is OpenResult.Failed -> errorObservation(TestStage.READ_TIMEOUT_ACCURACY, r.error, at)
            is OpenResult.Opened -> {
                val session = r.session
                val timeouts = ArrayList<Double>()
                var dataEvents = 0
                var otherErrors = 0
                try {
                    repeat(samples) {
                        currentCoroutineContext().ensureActive()
                        when (val o = session.read()) {
                            is ReadOutcome.TimedOut -> timeouts.add(o.elapsedMs)
                            is ReadOutcome.Data -> dataEvents++ // count only; content never read
                            is ReadOutcome.Failed -> otherErrors++
                        }
                    }
                } finally {
                    session.close()
                }
                val stats = Percentiles.summarize(timeouts)
                HardwareObservation(
                    stage = TestStage.READ_TIMEOUT_ACCURACY,
                    status = ObservationStatus.OBSERVED,
                    detail = "target ${timeoutMs}ms; timeout_samples=${timeouts.size} " +
                        "data_events=$dataEvents other_errors=$otherErrors",
                    timeoutStats = stats,
                    atElapsedMillis = at,
                )
            }
        }
    }

    /** Reads until a non-timeout outcome appears (e.g. an unplug), then classifies it. */
    suspend fun unplugDetection(target: Openable, timeoutMs: Int, maxSlices: Int): HardwareObservation {
        val at = clock()
        return when (val r = target.open(115_200, timeoutMs)) {
            is OpenResult.Failed -> errorObservation(TestStage.UNPLUG_DETECTION, r.error, at)
            is OpenResult.Opened -> {
                val session = r.session
                try {
                    repeat(maxSlices) {
                        currentCoroutineContext().ensureActive()
                        when (val o = session.read()) {
                            is ReadOutcome.TimedOut -> Unit // keep waiting for the unplug
                            is ReadOutcome.Data -> Unit     // still connected; count not needed here
                            is ReadOutcome.Failed -> return HardwareObservation(
                                stage = TestStage.UNPLUG_DETECTION,
                                status = ObservationStatus.CLASSIFIED_ERROR,
                                detail = "surfaced ${o.error.error} on read; record what you saw physically",
                                error = o.error.error,
                                atElapsedMillis = at,
                            )
                        }
                    }
                } finally {
                    session.close()
                }
                HardwareObservation(
                    stage = TestStage.UNPLUG_DETECTION,
                    status = ObservationStatus.OBSERVED,
                    detail = "no unplug observed within $maxSlices slices",
                    atElapsedMillis = at,
                )
            }
        }
    }

    private fun errorObservation(stage: TestStage, error: ClassifiedError, at: Long) =
        HardwareObservation(
            stage = stage,
            status = ObservationStatus.CLASSIFIED_ERROR,
            detail = "open failed: ${error.error}",
            error = error.error,
            atElapsedMillis = at,
        )
}
