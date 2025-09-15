// Debug script to test Soniox integration
// This can be pasted in the browser's developer console

console.log("🔍 Soniox Debug Tool Started");

// Monitor Soniox events
const events = ['soniox-transcript', 'soniox-error', 'soniox-status', 'soniox-bytes'];

events.forEach(eventName => {
    window.__TAURI__.event.listen(eventName, (event) => {
        console.log(`📡 ${eventName}:`, event.payload);

        if (eventName === 'soniox-transcript' && event.payload) {
            // Also log to a visible element for easier debugging
            const transcriptEl = document.getElementById('transcript');
            if (transcriptEl) {
                console.log(`📝 Transcript updated: ${event.payload.length} chars`);
            }
        }
    });
});

// Monitor audio levels to verify microphone is working
window.__TAURI__.event.listen('audio-level', (event) => {
    if (event.payload.peak > 0.1) {
        console.log(`🎤 Audio detected - Peak: ${event.payload.peak.toFixed(3)}, RMS: ${event.payload.rms.toFixed(3)}`);
    }
});

// Helper function to manually test Soniox
window.testSoniox = async function() {
    console.log("🧪 Testing Soniox integration...");

    const apiKeyInput = document.getElementById('soniox-api');
    const enableCheckbox = document.getElementById('soniox-enable');
    const transcriptEl = document.getElementById('transcript');

    if (!apiKeyInput.value.trim()) {
        console.error("❌ No API key found. Please enter your Soniox API key.");
        return;
    }

    try {
        // Clear transcript
        if (transcriptEl) transcriptEl.textContent = "Waiting for transcript...";

        // Enable Soniox
        enableCheckbox.checked = true;
        enableCheckbox.dispatchEvent(new Event('change'));

        console.log("✅ Soniox enabled. Start recording to test transcription.");

    } catch (error) {
        console.error("❌ Error testing Soniox:", error);
    }
};

// Helper to check current state
window.checkSonioxState = function() {
    const apiKeyInput = document.getElementById('soniox-api');
    const enableCheckbox = document.getElementById('soniox-enable');
    const statusEl = document.getElementById('soniox-status');
    const transcriptEl = document.getElementById('transcript');

    console.log("📊 Soniox State Check:");
    console.log("  API Key:", apiKeyInput.value ? "✅ Present" : "❌ Missing");
    console.log("  Enabled:", enableCheckbox.checked ? "✅ Yes" : "❌ No");
    console.log("  Status:", statusEl.textContent);
    console.log("  Status Class:", statusEl.className);
    console.log("  Transcript Content:", transcriptEl.textContent || "(empty)");
};

console.log("🎯 Debug commands available:");
console.log("  - testSoniox() : Enable Soniox and prepare for testing");
console.log("  - checkSonioxState() : Check current Soniox configuration");
console.log("  Events are being monitored automatically.");