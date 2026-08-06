package com.autonomousdroneexpert.m1c.domain

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive

/**
 * Runs the read-only hardware stages against any [Openable] (real Android USB device in
 * the app; a fake in tests). Android-independent so it is fully JVM unit-testable.
 *
 * Cancellation: these are suspend functions that check coroutine cancellation between
 * reads via [ensureActive]. **Coroutine cancellation is NOT proof that the underlying
 * driver I/O was cancelled** -- an in-flight blocking read may still be running in the
 * platform layer until its own timeout. This distinction is deliberate and documented.
 * [CancellationException] is always re-thrown, never swallowed or mis-classified.
 *
 * @param clock injected elapsed-millis source so tests are deterministic (no wall clock).
 */
class ReadOnlyHardwareTestRunner(private val clock: () -> Long) {

    companion object {
        /** Default single-open dwell: hold the port open long enough to watch the device. */
        const val DEFAULT_SINGLE_OPEN_DWELL_MS = 3_000L
    }

    /**
     * Open once, then **hold the port open** for [dwellMs] before closing -- long enough for
     * the operator to watch LEDs / re-enumeration -- instead of an instantaneous open/close.
     * Records open latency, dwell, and the close outcome. Never a PASS: HARDWARE_OBSERVED only.
     *
     * @param onPortOpen invoked once the port is open and before the dwell, so the UI can
     *   show "the port is open now -- watch the device".
     */
    suspend fun singleOpen(
        target: Openable,
        baud: Int,
        timeoutMs: Int,
        dwellMs: Long = DEFAULT_SINGLE_OPEN_DWELL_MS,
        onPortOpen: (() -> Unit)? = null,
    ): HardwareObservation {
        val at = clock()
        val openStart = clock()
        return when (val r = safeOpen(target, baud, timeoutMs)) {
            is OpenResult.Opened -> {
                val openLatencyMs = clock() - openStart
                onPortOpen?.invoke()
                val session = r.session
                var dwellElapsedMs = 0L
                var closeOutcome = "not_closed"
                try {
                    val dwellStart = clock()
                    // Real dwell: a cooperative suspension point, so Stop still cancels it.
                    delay(dwellMs)
                    dwellElapsedMs = clock() - dwellStart
                } finally {
                    closeOutcome = try {
                        session.close(); "clean"
                    } catch (t: Throwable) {
                        "close_error:${TransportClassifiers.classifyThrowable(t).error}"
                    }
                }
                HardwareObservation(
                    stage = TestStage.SINGLE_OPEN,
                    status = ObservationStatus.OBSERVED,
                    detail = "port held open then closed; open_latency=${openLatencyMs}ms " +
                        "dwell=${dwellElapsedMs}ms close=$closeOutcome; " +
                        "observe LED/COM/DFU/behaviour on the device",
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
            when (val r = safeOpen(target, baud, timeoutMs)) {
                is OpenResult.Opened -> { closeQuietly(r.session); clean++ }
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
        return when (val r = safeOpen(target, 115_200, timeoutMs)) {
            is OpenResult.Failed -> errorObservation(TestStage.READ_TIMEOUT_ACCURACY, r.error, at)
            is OpenResult.Opened -> {
                val session = r.session
                val timeouts = ArrayList<Double>()
                var dataEvents = 0
                var inferred = 0
                var otherErrors = 0
                var firstError: ClassifiedError? = null
                try {
                    repeat(samples) {
                        currentCoroutineContext().ensureActive()
                        when (val o = safeRead(session)) {
                            is ReadOutcome.TimedOut -> { timeouts.add(o.elapsedMs); if (o.inferred) inferred++ }
                            is ReadOutcome.Data -> dataEvents++ // count only; content never read
                            is ReadOutcome.Failed -> { otherErrors++; if (firstError == null) firstError = o.error }
                        }
                    }
                } finally {
                    closeQuietly(session)
                }
                val stats = Percentiles.summarize(timeouts)
                HardwareObservation(
                    stage = TestStage.READ_TIMEOUT_ACCURACY,
                    // Errors must not hide behind a clean-looking result.
                    status = if (otherErrors > 0) ObservationStatus.OBSERVED_WITH_ERRORS else ObservationStatus.OBSERVED,
                    detail = "target ${timeoutMs}ms; timeout_samples=${timeouts.size} inferred=$inferred " +
                        "data_events=$dataEvents other_errors=$otherErrors" +
                        (firstError?.let { "; first_error=${it.error}: ${it.originalMessage}" } ?: "") +
                        "; basis: non-positive bulkTransfer is ambiguous on Android; inferred timeouts are labelled",
                    timeoutStats = stats,
                    error = firstError?.error,
                    atElapsedMillis = at,
                )
            }
        }
    }

    /** Reads until a non-timeout error appears (e.g. an unplug), recording slice + total timing. */
    suspend fun unplugDetection(target: Openable, timeoutMs: Int, maxSlices: Int): HardwareObservation {
        val at = clock()
        return when (val r = safeOpen(target, 115_200, timeoutMs)) {
            is OpenResult.Failed -> errorObservation(TestStage.UNPLUG_DETECTION, r.error, at)
            is OpenResult.Opened -> {
                val session = r.session
                val startedAt = clock()
                var slices = 0
                try {
                    repeat(maxSlices) {
                        currentCoroutineContext().ensureActive()
                        slices++
                        when (val o = safeRead(session)) {
                            is ReadOutcome.TimedOut -> Unit // keep waiting for the unplug
                            is ReadOutcome.Data -> Unit     // still connected; count not needed here
                            is ReadOutcome.Failed -> {
                                val totalMs = clock() - startedAt
                                return HardwareObservation(
                                    stage = TestStage.UNPLUG_DETECTION,
                                    status = ObservationStatus.CLASSIFIED_ERROR,
                                    detail = "surfaced ${o.error.error} on read after $slices slice(s); " +
                                        "slice_elapsed=${o.elapsedMs}ms total=${totalMs}ms; " +
                                        "basis=${o.error.originalMessage}; record what you saw physically",
                                    error = o.error.error,
                                    atElapsedMillis = at,
                                )
                            }
                        }
                    }
                } finally {
                    closeQuietly(session)
                }
                val totalMs = clock() - startedAt
                HardwareObservation(
                    stage = TestStage.UNPLUG_DETECTION,
                    status = ObservationStatus.OBSERVED,
                    detail = "no unplug observed within $maxSlices slices; total=${totalMs}ms",
                    atElapsedMillis = at,
                )
            }
        }
    }

    /** Open, mapping any unexpected throwable (never a [CancellationException]) to UNKNOWN_IO_ERROR. */
    private suspend fun safeOpen(target: Openable, baud: Int, timeoutMs: Int): OpenResult =
        try {
            target.open(baud, timeoutMs)
        } catch (e: CancellationException) {
            throw e
        } catch (t: Throwable) {
            OpenResult.Failed(TransportClassifiers.classifyThrowable(t))
        }

    /** Read, mapping any unexpected throwable (never a [CancellationException]) to UNKNOWN_IO_ERROR. */
    private suspend fun safeRead(session: ReadOnlySession): ReadOutcome =
        try {
            session.read()
        } catch (e: CancellationException) {
            throw e
        } catch (t: Throwable) {
            ReadOutcome.Failed(TransportClassifiers.classifyThrowable(t))
        }

    /** Close in a finally without masking the stage result; the single-open stage records its own outcome. */
    private fun closeQuietly(session: ReadOnlySession) {
        try {
            session.close()
        } catch (_: Throwable) {
            // A close failure must not mask a stage's read/open evidence.
        }
    }

    private fun errorObservation(stage: TestStage, error: ClassifiedError, at: Long) =
        HardwareObservation(
            stage = stage,
            status = ObservationStatus.CLASSIFIED_ERROR,
            detail = "open failed: ${error.error}: ${error.originalMessage}",
            error = error.error,
            atElapsedMillis = at,
        )
}
