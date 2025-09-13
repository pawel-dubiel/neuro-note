import { invoke } from "@tauri-apps/api/core";
import { documentDir, join } from "@tauri-apps/api/path";
import { listen } from "@tauri-apps/api/event";

let btnStart: HTMLButtonElement | null;
let btnStop: HTMLButtonElement | null;
let statusEl: HTMLElement | null;
let lastSavedEl: HTMLElement | null;
let meterFillEl: HTMLElement | null;
let formatSel: HTMLSelectElement | null;
let qualitySel: HTMLSelectElement | null;
let autoDetectChk: HTMLInputElement | null;
let currentPath: string | null = null;
let isVoiceMode = false;

async function start() {
  if (!btnStart || !btnStop || !statusEl) return;
  try {
    btnStart.disabled = true;
    statusEl.textContent = "Starting…";

    if (autoDetectChk?.checked) {
      // Voice activation mode
      isVoiceMode = true;
      await invoke("arm_auto_recording", {
        threshold: 0.01,     // Very low threshold for maximum sensitivity
        minSpeechMs: 100,    // Very short duration to trigger easily
        silenceMs: 1000,     // 1 second of silence to stop recording
        preRollMs: 250,      // 250ms pre-roll buffer
      });
      statusEl.textContent = "Listening for voice…";
      btnStop.disabled = false;
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
      btnStop.disabled = false;
    }
  } catch (e: any) {
    statusEl.textContent = `Error: ${e}`;
    btnStart.disabled = false;
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
  } catch (e: any) {
    statusEl.textContent = `Error: ${e}`;
  } finally {
    btnStart.disabled = false;
    btnStop.disabled = true;
  }
}

window.addEventListener("DOMContentLoaded", () => {
  btnStart = document.querySelector("#btn-start");
  btnStop = document.querySelector("#btn-stop");
  statusEl = document.querySelector("#status");
  lastSavedEl = document.querySelector("#last-saved");
  meterFillEl = document.querySelector("#meter-fill");
  formatSel = document.querySelector("#format");
  qualitySel = document.querySelector("#quality");
  autoDetectChk = document.querySelector("#auto");

  btnStart?.addEventListener("click", start);
  btnStop?.addEventListener("click", stop);
  
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
    if (lastSavedEl && statusEl && isVoiceMode) {
      lastSavedEl.textContent = `Saved: ${event.payload}`;
      statusEl.textContent = "Listening for voice…";
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
