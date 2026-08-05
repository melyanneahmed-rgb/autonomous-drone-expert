package com.autonomousdroneexpert.m1c

import android.os.Bundle
import android.os.SystemClock
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.runtime.CompositionLocalProvider
import com.autonomousdroneexpert.m1c.domain.SafetyAttestation
import com.autonomousdroneexpert.m1c.ui.IdentificationScreen
import com.autonomousdroneexpert.m1c.ui.M1CTheme
import com.autonomousdroneexpert.m1c.ui.SafetyGateScreen
import com.autonomousdroneexpert.m1c.ui.Screen

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            M1CTheme {
                // Arabic-first: force RTL layout direction for the whole app.
                CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Rtl) {
                    Surface(modifier = Modifier.fillMaxSize()) {
                        AppRoot()
                    }
                }
            }
        }
    }
}

@androidx.compose.runtime.Composable
private fun AppRoot() {
    var screen by remember { mutableStateOf(Screen.Identification) }
    var attestation by remember { mutableStateOf(SafetyAttestation()) }

    Box(Modifier.fillMaxSize()) {
        when (screen) {
            Screen.Identification -> IdentificationScreen(onContinue = { screen = Screen.SafetyGate })
            Screen.SafetyGate -> SafetyGateScreen(
                attestation = attestation,
                onToggle = { item, on ->
                    attestation = attestation.toggle(item, on, SystemClock.elapsedRealtime())
                },
                onProceed = { screen = Screen.Devices },
            )
            // Devices/Tests/Results are wired in the harness commit; foundation stops here.
            Screen.Devices, Screen.Tests, Screen.Results ->
                IdentificationScreen(onContinue = { screen = Screen.Identification })
        }
    }
}
