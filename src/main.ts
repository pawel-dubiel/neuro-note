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
let savedVoiceConfig: {
  threshold: number;
  minSpeechMs: number;
  silenceMs: number;
  preRollMs: number;
  format: string;
  quality: string;
} | null = null;

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

      // Save config for resume
      savedVoiceConfig = {
        threshold: 0.01,
        minSpeechMs: 100,
        silenceMs: 1000,
        preRollMs: 250,
        format,
        quality,
      };

      await invoke("arm_auto_recording", savedVoiceConfig);
      statusEl.textContent = "Listening for voice…";
      updateUIForState({ type: "Recording", data: { start_time: new Date().toISOString(), elapsed_ms: 0 } });
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
      // Pause voice-activated mode by disarming VAD
      await invoke("disarm_auto_recording");
      statusEl.textContent = "Paused";
      updateUIForState({ type: "Paused", data: { pause_time: new Date().toISOString(), elapsed_ms: 0 } });
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
      // Resume voice-activated mode by re-arming VAD with saved config
      const cfg = savedVoiceConfig || {
        threshold: 0.01,
        minSpeechMs: 100,
        silenceMs: 1000,
        preRollMs: 250,
        format: (formatSel?.value || "wav").toLowerCase(),
        quality: (qualitySel?.value || "high").toLowerCase(),
      };
      await invoke("arm_auto_recording", cfg);
      statusEl.textContent = "Listening for voice…";
      updateUIForState({ type: "Recording", data: { start_time: new Date().toISOString(), elapsed_ms: 0 } });
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
      // Stop voice activation mode: disarm then finalize single file
      try { await invoke("disarm_auto_recording"); } catch {}
      const saved = await invoke<string>("finalize_auto_recording");
      statusEl.textContent = "Idle";
      lastSavedEl.textContent = `Saved: ${saved}`;
      isVoiceMode = false;
      savedVoiceConfig = null;
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

  const setConfigControlsEnabled = (enabled: boolean) => {
    if (formatSel) formatSel.disabled = !enabled;
    if (qualitySel) {
      // When enabled, only allow editing for MP3; otherwise keep disabled
      if (enabled) {
        const isMp3 = formatSel?.value === "mp3";
        qualitySel.disabled = !isMp3;
      } else {
        qualitySel.disabled = true;
      }
      (document.querySelector("#quality-wrap") as HTMLElement)?.classList.toggle("disabled", qualitySel.disabled);
    }
  };
  const setModeControlEnabled = (enabled: boolean) => {
    if (autoDetectChk) autoDetectChk.disabled = !enabled;
  };

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
      setConfigControlsEnabled(true);
      setModeControlEnabled(true);
      break;
    case "Starting":
    case "Recording":
      btnStart.disabled = true;
      btnPause.disabled = false;
      btnStop.disabled = false;
      setConfigControlsEnabled(false);
      setModeControlEnabled(false);
      break;
    case "Paused":
      btnStart.disabled = true;
      btnPause.style.display = "none";
      btnResume.style.display = "inline-block";
      btnResume.disabled = false;
      btnStop.disabled = false;
      setConfigControlsEnabled(false);
      setModeControlEnabled(false);
      break;
    case "Resuming":
      btnStart.disabled = true;
      btnStop.disabled = false;
      setConfigControlsEnabled(false);
      setModeControlEnabled(false);
      break;
    case "Stopping":
      btnStart.disabled = true;
      setConfigControlsEnabled(false);
      setModeControlEnabled(false);
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

  // No special disabling; pause/resume supported for both modes now

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
