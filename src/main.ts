import { invoke } from "@tauri-apps/api/core";
import { documentDir, join } from "@tauri-apps/api/path";
import { listen } from "@tauri-apps/api/event";

let btnStart: HTMLButtonElement | null;
let btnPause: HTMLButtonElement | null;
let btnResume: HTMLButtonElement | null;
let btnStop: HTMLButtonElement | null;
let statusEl: HTMLElement | null;
let lastSavedEl: HTMLElement | null;
let meterFillEl: HTMLElement | null;
let formatSel: HTMLSelectElement | null;
let qualitySel: HTMLSelectElement | null;
let autoDetectChk: HTMLInputElement | null;
let currentPath: string | null = null;
let isVoiceMode = false;

// Recording state management
type RecordingState =
  | { type: "Idle" }
  | { type: "Starting" }
  | { type: "Recording", data: { start_time: string, elapsed_ms: number } }
  | { type: "Paused", data: { pause_time: string, elapsed_ms: number } }
  | { type: "Resuming" }
  | { type: "Stopping" };

// Current recording state is managed by the backend now

async function start() {
  if (!btnStart || !btnStop || !statusEl) return;
  try {
    updateUIForState({ type: "Starting" });
    statusEl.textContent = "Starting…";

    if (autoDetectChk?.checked) {
      // Voice activation mode
      isVoiceMode = true;
      const format = (formatSel?.value || "wav").toLowerCase();
      const quality = (qualitySel?.value || "high").toLowerCase();

      await invoke("arm_auto_recording", {
        threshold: 0.01,     // Very low threshold for maximum sensitivity
        minSpeechMs: 100,    // Very short duration to trigger easily
        silenceMs: 1000,     // 1 second of silence to stop recording
        preRollMs: 250,      // 250ms pre-roll buffer
        format,
        quality,
      });
      statusEl.textContent = "Listening for voice…";
      // Note: Voice mode doesn't support pause/resume yet
      updateUIForState({ type: "Recording", data: { start_time: new Date().toISOString(), elapsed_ms: 0 } });
      if (btnPause) btnPause.disabled = true; // Disable pause for voice mode
    } else {
      // Manual recording mode
      isVoiceMode = false;
      const dir = await documentDir();
      const format = (formatSel?.value || "wav").toLowerCase();
      const file = `recording-${Date.now()}.${format}`;
      const out = await join(dir, file);
      const quality = (qualitySel?.value || "high").toLowerCase();

      currentPath = await invoke<string>("start_recording", {
        path: out,
        format,
        quality,
      });
      statusEl.textContent = `Recording… (${currentPath})`;
      updateUIForState({ type: "Recording", data: { start_time: new Date().toISOString(), elapsed_ms: 0 } });
    }
  } catch (e: any) {
    statusEl.textContent = `Error: ${e}`;
    updateUIForState({ type: "Idle" });
  }
}

async function pause() {
  if (!btnPause || !btnResume || !statusEl) return;
  try {
    btnPause.disabled = true;
    statusEl.textContent = "Pausing…";

    if (isVoiceMode) {
      // For voice mode, we'll need to implement pause in the auto-recording system
      // For now, show that it's not supported
      statusEl.textContent = "Pause not supported in voice mode";
      btnPause.disabled = false;
      return;
    } else {
      // Pause manual recording
      await invoke<string>("pause_recording");
      statusEl.textContent = "Paused";
      updateUIForState({ type: "Paused", data: { pause_time: new Date().toISOString(), elapsed_ms: 0 } });
    }
  } catch (e: any) {
    statusEl.textContent = `Error: ${e}`;
    btnPause.disabled = false;
  }
}

async function resume() {
  if (!btnPause || !btnResume || !statusEl) return;
  try {
    btnResume.disabled = true;
    statusEl.textContent = "Resuming…";

    if (isVoiceMode) {
      // For voice mode, we'll need to implement resume in the auto-recording system
      statusEl.textContent = "Resume not supported in voice mode";
      btnResume.disabled = false;
      return;
    } else {
      // Resume manual recording
      await invoke<string>("resume_recording");
      statusEl.textContent = "Recording…";
      updateUIForState({ type: "Recording", data: { start_time: new Date().toISOString(), elapsed_ms: 0 } });
    }
  } catch (e: any) {
    statusEl.textContent = `Error: ${e}`;
    btnResume.disabled = false;
  }
}

async function stop() {
  if (!btnStart || !btnStop || !statusEl || !lastSavedEl) return;
  try {
    btnStop.disabled = true;
    statusEl.textContent = "Stopping…";

    if (isVoiceMode) {
      // Stop voice activation mode
      await invoke("disarm_auto_recording");
      statusEl.textContent = "Idle";
      isVoiceMode = false;
    } else {
      // Stop manual recording
      const saved = await invoke<string>("stop_recording");
      statusEl.textContent = "Idle";
      lastSavedEl.textContent = `Saved: ${saved}`;
      currentPath = null;
    }
    updateUIForState({ type: "Idle" });
  } catch (e: any) {
    statusEl.textContent = `Error: ${e}`;
  } finally {
    btnStart.disabled = false;
    btnStop.disabled = true;
  }
}

function updateUIForState(state: RecordingState) {
  // Update UI based on current recording state

  if (!btnStart || !btnPause || !btnResume || !btnStop) return;

  // Reset all buttons
  btnStart.disabled = false;
  btnPause.disabled = true;
  btnResume.disabled = true;
  btnStop.disabled = true;

  // Hide resume button by default
  btnResume.style.display = "none";
  btnPause.style.display = "inline-block";

  switch (state.type) {
    case "Idle":
      btnStart.disabled = false;
      break;
    case "Starting":
    case "Recording":
      btnStart.disabled = true;
      btnPause.disabled = false;
      btnStop.disabled = false;
      break;
    case "Paused":
      btnStart.disabled = true;
      btnPause.style.display = "none";
      btnResume.style.display = "inline-block";
      btnResume.disabled = false;
      btnStop.disabled = false;
      break;
    case "Resuming":
      btnStart.disabled = true;
      btnStop.disabled = false;
      break;
    case "Stopping":
      btnStart.disabled = true;
      break;
  }
}

window.addEventListener("DOMContentLoaded", () => {
  btnStart = document.querySelector("#btn-start");
  btnPause = document.querySelector("#btn-pause");
  btnResume = document.querySelector("#btn-resume");
  btnStop = document.querySelector("#btn-stop");
  statusEl = document.querySelector("#status");
  lastSavedEl = document.querySelector("#last-saved");
  meterFillEl = document.querySelector("#meter-fill");
  formatSel = document.querySelector("#format");
  qualitySel = document.querySelector("#quality");
  autoDetectChk = document.querySelector("#auto");

  btnStart?.addEventListener("click", start);
  btnPause?.addEventListener("click", pause);
  btnResume?.addEventListener("click", resume);
  btnStop?.addEventListener("click", stop);

  // Listen for recording state changes from backend
  listen<RecordingState>("recording-state-changed", (event) => {
    updateUIForState(event.payload);
    if (statusEl) {
      switch (event.payload.type) {
        case "Idle":
          statusEl.textContent = "Idle";
          break;
        case "Recording":
          statusEl.textContent = "Recording…";
          break;
        case "Paused":
          statusEl.textContent = "Paused";
          break;
        case "Starting":
          statusEl.textContent = "Starting…";
          break;
        case "Resuming":
          statusEl.textContent = "Resuming…";
          break;
        case "Stopping":
          statusEl.textContent = "Stopping…";
          break;
      }
    }
  });

  // Audio level meter
  listen<{ rms: number; peak: number }>("audio-level", (event) => {
    const { peak } = event.payload;
    if (meterFillEl) {
      const pct = Math.max(0, Math.min(100, Math.round(peak * 100)));
      meterFillEl.style.width = pct + "%";
    }
  });

  // Voice activity detection events
  listen<string>("vad-segment-start", () => {
    if (statusEl && isVoiceMode) {
      statusEl.textContent = "🎤 Recording voice…";
    }
  });

  listen<string>("vad-segment-saved", (event) => {
    console.log("VAD segment saved:", event.payload);
    if (lastSavedEl && statusEl) {
      lastSavedEl.textContent = `Saved: ${event.payload}`;
      if (isVoiceMode) {
        statusEl.textContent = "Listening for voice…";
      }
    }
  });

  listen<string>("vad-threshold", (event) => {
    console.log(`Voice threshold calibrated to: ${event.payload}`);
  });

  listen<string>("vad-error", (event) => {
    if (statusEl) {
      statusEl.textContent = `Error: ${event.payload}`;
      isVoiceMode = false;
      if (btnStart) btnStart.disabled = false;
      if (btnStop) btnStop.disabled = true;
    }
  });

  // Update button text based on auto detect mode
  autoDetectChk?.addEventListener("change", () => {
    if (btnStart) {
      btnStart.textContent = autoDetectChk?.checked ? 
        "Start Voice Detection" : "Start Recording";
    }
  });

  // Disable quality when WAV is selected
  formatSel?.addEventListener("change", () => {
    const isMp3 = formatSel?.value === "mp3";
    if (qualitySel) {
      qualitySel.disabled = !isMp3;
      (document.querySelector("#quality-wrap") as HTMLElement)?.classList.toggle("disabled", !isMp3);
    }
  });
  // Initialize state
  formatSel?.dispatchEvent(new Event("change"));
});
