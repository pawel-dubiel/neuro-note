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
let sonioxApiKeyInp: HTMLInputElement | null;
let sonioxEnableChk: HTMLInputElement | null;
let sonioxStatusEl: HTMLElement | null;
let sonioxConnected = false;
let openaiApiKeyInp: HTMLInputElement | null;
let openaiModelSel: HTMLSelectElement | null;
let openaiEnableChk: HTMLInputElement | null;
let openaiStatusEl: HTMLElement | null;
let aiAnalysisEl: HTMLElement | null;
let lastTranscript = "";
let currentStateType: RecordingState["type"] = "Idle";
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

      // Start Soniox session for voice detection mode if enabled
      if (sonioxEnableChk?.checked && !sonioxConnected) {
        const api_key = (sonioxApiKeyInp?.value || "").trim();
        if (api_key) {
          try {
            if (sonioxStatusEl) {
              sonioxStatusEl.textContent = "Connecting…";
              sonioxStatusEl.className = "soniox-status connecting";
            }
            await invoke("start_soniox_session", { opts: { api_key, audio_format: "pcm_s16le", translation: "none" } });
          } catch (e: any) {
            console.error("Soniox start error in voice mode:", e);
          }
        }
      }
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

      // Restart Soniox session for voice detection mode if enabled
      if (sonioxEnableChk?.checked && !sonioxConnected) {
        const api_key = (sonioxApiKeyInp?.value || "").trim();
        if (api_key) {
          try {
            if (sonioxStatusEl) {
              sonioxStatusEl.textContent = "Connecting…";
              sonioxStatusEl.className = "soniox-status connecting";
            }
            await invoke("start_soniox_session", { opts: { api_key, audio_format: "pcm_s16le", translation: "none" } });
          } catch (e: any) {
            console.error("Soniox restart error in voice mode:", e);
          }
        }
      }
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

// OpenAI analysis function
let analyzing = false;
async function analyzeWithOpenAI() {
  if (analyzing || !openaiApiKeyInp?.value.trim() || !lastTranscript.trim()) {
    return;
  }

  analyzing = true;

  try {
    // Update status indicator instead of AI analysis content
    if (openaiStatusEl) {
      openaiStatusEl.textContent = "Analyzing...";
      openaiStatusEl.classList.remove("ready", "error");
      openaiStatusEl.classList.add("analyzing");
    }

    const selectedModel = openaiModelSel?.value || "gpt-4.1";
    const result = await invoke<string>("analyze_with_openai", {
      transcript: lastTranscript,
      apiKey: openaiApiKeyInp.value.trim(),
      model: selectedModel
    });

    if (aiAnalysisEl) {
      aiAnalysisEl.textContent = result;
      aiAnalysisEl.scrollTop = aiAnalysisEl.scrollHeight;
    }

    if (openaiStatusEl) {
      openaiStatusEl.textContent = "Ready";
      openaiStatusEl.classList.remove("analyzing", "error");
      openaiStatusEl.classList.add("ready");
    }

    console.log("✅ OpenAI analysis completed");
  } catch (error) {
    console.error("❌ OpenAI analysis error:", error);

    if (openaiStatusEl) {
      openaiStatusEl.textContent = "Error";
      openaiStatusEl.classList.remove("analyzing", "ready");
      openaiStatusEl.classList.add("error");
    }

    if (aiAnalysisEl) {
      aiAnalysisEl.textContent = `Error: ${error}`;
    }
  } finally {
    analyzing = false;
  }
}

// Function to fetch and populate OpenAI models
async function fetchOpenAIModels() {
  if (!openaiApiKeyInp?.value.trim() || !openaiModelSel) {
    return;
  }

  try {
    const models = await invoke<string[]>("get_openai_models", {
      apiKey: openaiApiKeyInp.value.trim()
    });

    // Clear existing options except the first default ones
    const currentValue = openaiModelSel.value;
    openaiModelSel.innerHTML = "";

    // Add fetched models
    models.forEach(model => {
      const option = document.createElement("option");
      option.value = model;
      option.textContent = model;
      openaiModelSel?.appendChild(option);
    });

    // Try to restore previous selection, or select first model
    if (models.includes(currentValue)) {
      openaiModelSel.value = currentValue;
    } else if (models.length > 0) {
      openaiModelSel.value = models[0];
    }

    console.log(`✅ Loaded ${models.length} OpenAI models`);
  } catch (error) {
    console.error("❌ Failed to fetch OpenAI models:", error);
    // Keep default options if fetching fails
  }
}

// Panel toggle functionality
function setupPanelToggles() {
  const transcriptToggle = document.querySelector("#toggle-transcript");
  const aiToggle = document.querySelector("#toggle-ai");
  const transcriptPanel = document.querySelector("#transcript-panel");
  const aiPanel = document.querySelector("#ai-panel");

  // Initially show transcript panel, hide AI panel
  transcriptPanel?.classList.remove("hidden");
  aiPanel?.classList.add("hidden");

  transcriptToggle?.addEventListener("click", () => {
    const isHidden = transcriptPanel?.classList.contains("hidden");
    if (isHidden) {
      transcriptPanel?.classList.remove("hidden");
      transcriptToggle.classList.add("active");
    } else {
      transcriptPanel?.classList.add("hidden");
      transcriptToggle.classList.remove("active");
    }
  });

  aiToggle?.addEventListener("click", () => {
    const isHidden = aiPanel?.classList.contains("hidden");
    if (isHidden) {
      aiPanel?.classList.remove("hidden");
      aiToggle.classList.add("active");
    } else {
      aiPanel?.classList.add("hidden");
      aiToggle.classList.remove("active");
    }
  });

  // Panel close buttons
  document.querySelectorAll(".panel-close").forEach(btn => {
    btn.addEventListener("click", () => {
      const panelType = btn.getAttribute("data-panel");
      if (panelType === "transcript") {
        transcriptPanel?.classList.add("hidden");
        transcriptToggle?.classList.remove("active");
      } else if (panelType === "ai") {
        aiPanel?.classList.add("hidden");
        aiToggle?.classList.remove("active");
      }
    });
  });
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
  sonioxApiKeyInp = document.querySelector("#soniox-api");
  sonioxEnableChk = document.querySelector("#soniox-enable");
  sonioxStatusEl = document.querySelector("#soniox-status");
  openaiApiKeyInp = document.querySelector("#openai-api");
  openaiModelSel = document.querySelector("#openai-model");
  openaiEnableChk = document.querySelector("#openai-enable");
  openaiStatusEl = document.querySelector("#openai-status");
  aiAnalysisEl = document.querySelector("#ai-analysis");

  // Panel toggle functionality
  setupPanelToggles();

  btnStart?.addEventListener("click", start);
  btnPause?.addEventListener("click", pause);
  btnResume?.addEventListener("click", resume);
  btnStop?.addEventListener("click", stop);

  // OpenAI enable checkbox listener
  openaiEnableChk?.addEventListener("change", () => {
    if (openaiEnableChk?.checked && lastTranscript.trim().length > 20) {
      analyzeWithOpenAI();
    }
  });

  // OpenAI API key input listener - fetch models when key is pasted/entered
  openaiApiKeyInp?.addEventListener("input", () => {
    // Debounce the API call
    clearTimeout((window as any).openaiModelTimeout);
    (window as any).openaiModelTimeout = setTimeout(() => {
      if (openaiApiKeyInp?.value.trim().length) {
        fetchOpenAIModels();
      }
    }, 1000);
  });

  // No special disabling; pause/resume supported for both modes now

  // Listen for recording state changes from backend
  listen<RecordingState>("recording-state-changed", (event) => {
    updateUIForState(event.payload);
    currentStateType = event.payload.type;
    if (statusEl) {
      switch (event.payload.type) {
        case "Idle":
          statusEl.textContent = "Idle";
          // Auto-stop Soniox when stream fully stops
          if (sonioxConnected) {
            invoke("stop_soniox_session").catch(() => {});
          }
          break;
        case "Recording":
          statusEl.textContent = "Recording…";
          // Auto-start Soniox if enabled and not connected
          if (sonioxEnableChk?.checked && !sonioxConnected) {
            const api_key = (sonioxApiKeyInp?.value || "").trim();
            invoke("start_soniox_session", { opts: { api_key, audio_format: "pcm_s16le", translation: "none" } }).catch((e) => {
              console.error("Soniox auto-start error:", e);
            });
          }
          break;
        case "Paused":
          statusEl.textContent = "Paused";
          // In voice mode, Paused disarms stream; stop Soniox to avoid 408
          if (isVoiceMode && sonioxConnected) {
            invoke("stop_soniox_session").catch(() => {});
          }
          break;
        case "Starting":
          statusEl.textContent = "Starting…";
          break;
        case "Resuming":
          statusEl.textContent = "Resuming…";
          if (sonioxEnableChk?.checked && !sonioxConnected) {
            const api_key = (sonioxApiKeyInp?.value || "").trim();
            invoke("start_soniox_session", { opts: { api_key, audio_format: "pcm_s16le", translation: "none" } }).catch((e) => {
              console.error("Soniox auto-start error:", e);
            });
          }
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

  // Soniox transcript events
  const transcriptEl = document.getElementById("transcript");
  listen<string>("soniox-transcript", (event) => {
    console.log("📝 Received soniox-transcript event:", event.payload);

    if (transcriptEl) {
      transcriptEl.textContent = event.payload;
      transcriptEl.scrollTop = transcriptEl.scrollHeight;
      console.log("✅ Updated transcript element with:", event.payload);
    } else {
      console.error("❌ Transcript element not found!");
    }

    // Store transcript for OpenAI analysis
    lastTranscript = event.payload;

    // Trigger OpenAI analysis if enabled and we have substantial content
    if (openaiEnableChk?.checked && lastTranscript.trim().length > 50) {
      analyzeWithOpenAI();
    }

    if (sonioxEnableChk?.checked && sonioxStatusEl && !sonioxConnected) {
      sonioxConnected = true;
      sonioxStatusEl.textContent = "Connected";
      sonioxStatusEl.classList.remove("connecting", "error", "off");
      sonioxStatusEl.classList.add("connected");
    }
  });
  listen<string>("soniox-error", (event) => {
    console.error("Soniox error:", event.payload);
    if (sonioxStatusEl) {
      sonioxStatusEl.textContent = "Error";
      sonioxStatusEl.classList.remove("connecting", "connected", "off");
      sonioxStatusEl.classList.add("error");
    }
    if (sonioxEnableChk) sonioxEnableChk.checked = false;
    sonioxConnected = false;
  });
  listen<string>("soniox-status", (event) => {
    const st = event.payload;
    if (!sonioxStatusEl) return;
    switch (st) {
      case "connecting":
        sonioxStatusEl.textContent = "Connecting…";
        sonioxStatusEl.className = "soniox-status connecting";
        break;
      case "connected":
        sonioxStatusEl.textContent = "Connected";
        sonioxStatusEl.className = "soniox-status connected";
        break;
      case "config_sent":
        // minor state; keep as connected
        sonioxStatusEl.textContent = "Connected";
        sonioxStatusEl.className = "soniox-status connected";
        break;
      case "finished":
      case "closed":
      case "ended":
        sonioxStatusEl.textContent = "Off";
        sonioxStatusEl.className = "soniox-status off";
        sonioxConnected = false;
        if (sonioxEnableChk) sonioxEnableChk.checked = false;
        break;
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

  // Soniox: load and persist API key
  const LS_KEY = "soniox_api_key";
  const savedKey = localStorage.getItem(LS_KEY) || "";
  if (sonioxApiKeyInp) sonioxApiKeyInp.value = savedKey;
  sonioxApiKeyInp?.addEventListener("input", () => {
    localStorage.setItem(LS_KEY, sonioxApiKeyInp!.value.trim());
  });

  // Soniox: toggle session from checkbox
  sonioxEnableChk?.addEventListener("change", async () => {
    if (!sonioxEnableChk) return;
    try {
      if (sonioxEnableChk.checked) {
        const api_key = (sonioxApiKeyInp?.value || "").trim();
        if (!api_key) {
          alert("Please enter your Soniox API key.");
          sonioxEnableChk.checked = false;
          return;
        }
        // Start immediately only if stream is active; otherwise wait for Recording
        if (currentStateType === "Recording" || currentStateType === "Resuming") {
          if (sonioxStatusEl) {
            sonioxStatusEl.textContent = "Connecting…";
            sonioxStatusEl.className = "soniox-status connecting";
          }
          sonioxConnected = false;
          await invoke("start_soniox_session", { opts: { api_key, audio_format: "pcm_s16le", translation: "none" } });
        } else {
          if (sonioxStatusEl) {
            sonioxStatusEl.textContent = "Off";
            sonioxStatusEl.className = "soniox-status off";
          }
        }
      } else {
        await invoke("stop_soniox_session");
        if (sonioxStatusEl) {
          sonioxStatusEl.textContent = "Off";
          sonioxStatusEl.classList.remove("connecting", "connected", "error");
          sonioxStatusEl.classList.add("off");
        }
        sonioxConnected = false;
      }
    } catch (e: any) {
      console.error("Soniox toggle error:", e);
      if (statusEl) statusEl.textContent = `Soniox: ${e}`;
      sonioxEnableChk.checked = false;
      if (sonioxStatusEl) {
        sonioxStatusEl.textContent = "Error";
        sonioxStatusEl.classList.remove("connecting", "connected", "off");
        sonioxStatusEl.classList.add("error");
      }
      sonioxConnected = false;
    }
  });
});
