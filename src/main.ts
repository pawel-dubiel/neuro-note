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
let assistantSel: HTMLSelectElement | null;
let openaiModelSel: HTMLSelectElement | null;
let openaiEnableChk: HTMLInputElement | null;
let openaiStatusEl: HTMLElement | null;
let aiAnalysisEl: HTMLElement | null;
let aiPrevBtn: HTMLButtonElement | null;
let aiNextBtn: HTMLButtonElement | null;
let aiPosEl: HTMLElement | null;
let aiAnswers: string[] = [];
let aiIndex: number = -1; // -1 means no history yet
let lastTranscript = "";
let lastAnalyzedStable = "";
let lastAnalysisAt = 0;
let gateCountEl: HTMLElement | null;
let gateRuns = 0;
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

// Load initial configuration on app startup
async function loadInitialConfiguration() {
  try {
    const config = await invoke<any>("load_app_config");

    // Update UI with loaded configuration
    if (sonioxApiKeyInp) sonioxApiKeyInp.value = config.soniox.api_key || "";
    if (sonioxEnableChk) sonioxEnableChk.checked = config.ui.enable_soniox || false;
    if (openaiApiKeyInp) openaiApiKeyInp.value = config.openai.api_key || "";
    if (openaiModelSel) openaiModelSel.value = config.openai.model || "gpt-4.1";
    if (openaiEnableChk) openaiEnableChk.checked = config.ui.enable_openai || false;
    if (formatSel) formatSel.value = config.recording.default_format || "mp3";
    if (qualitySel) qualitySel.value = config.recording.default_quality || "verylow";
    if (autoDetectChk) autoDetectChk.checked = config.recording.auto_detect_enabled !== false;
    if (assistantSel) assistantSel.value = config.ui.default_assistant || "general";

    console.log("✅ Initial configuration loaded successfully");

    // Fetch models if OpenAI API key is available
    if (config.openai.api_key) {
      await fetchOpenAIModels();
      // After fetching, ensure the configured model is selected
      if (openaiModelSel && config.openai.model) {
        openaiModelSel.value = config.openai.model;
      }
    }
  } catch (error) {
    console.log("ℹ️ No configuration found, using defaults:", error);
    // This is expected on first run, continue with defaults
  }
}

// Assistant management functions
async function loadAssistants() {
  try {
    console.log("Loading assistants...");
    await invoke("load_assistants");
    const assistants = await invoke<any[]>("get_assistants");
    const defaultId = await invoke<string>("get_default_assistant_id");

    console.log("Loaded assistants:", assistants);
    console.log("Default ID:", defaultId);

    if (assistantSel && assistants.length > 0) {
      // Clear loading option
      assistantSel.innerHTML = "";

      // Add assistants to selector
      assistants.forEach(assistant => {
        const option = document.createElement("option");
        option.value = assistant.id;
        option.textContent = assistant.name;
        option.title = assistant.description;
        assistantSel!.appendChild(option);
        console.log(`Added assistant: ${assistant.name} (${assistant.id})`);
      });

      // Select default assistant
      assistantSel.value = defaultId;
      console.log("Assistant selector populated successfully");
    } else {
      throw new Error("No assistants returned from backend");
    }
  } catch (e) {
    const errorMsg = `ASSISTANT LOADING FAILED: ${e}`;
    console.error(errorMsg);

    // Show error in UI
    if (assistantSel) {
      assistantSel.innerHTML = '<option value="">ERROR - Check config/assistants.json</option>';
      assistantSel.disabled = true;
      assistantSel.style.backgroundColor = '#ff6b6b';
      assistantSel.style.color = 'white';
    }

    // Also throw to prevent app from continuing with broken state
    throw new Error(errorMsg);
  }
}

// OpenAI analysis function
let analyzing = false;
async function analyzeWithOpenAI(transcriptOverride?: string) {
  const transcriptToAnalyze = (transcriptOverride ?? lastTranscript).trim();
  if (analyzing || !openaiApiKeyInp?.value.trim() || !transcriptToAnalyze) {
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
      transcript: transcriptToAnalyze,
      apiKey: openaiApiKeyInp.value.trim(),
      model: selectedModel,
      assistantId: assistantSel?.value || null
    });

    pushAiAnswer(result);

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

// Heuristic gating helpers
const MIN_ANALYSIS_INTERVAL_MS = 4000; // throttle expensive calls
const MIN_NEW_CHARS = 30;              // require meaningful delta

function stripTentative(text: string): string {
  // Remove segments formatted as tentative (rendered with underscores)
  // Keep it conservative to avoid over-removal
  return text.replace(/_+[^_]*_+/g, "");
}

function isStable(text: string): boolean {
  // Consider stable if it ends with sentence punctuation and has no visible tentative segment
  const ends = /[.!?)]\s*$/.test(text.trim());
  const hasTentative = /_+[^_]*_+/.test(text);
  return ends && !hasTentative;
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

// Configuration modal functionality
async function setupConfigModal() {
  const configBtn = document.querySelector("#config-btn");
  const configModal = document.querySelector("#config-modal");
  const configClose = document.querySelector("#config-close");
  const configSave = document.querySelector("#config-save");
  const configCancel = document.querySelector("#config-cancel");

  // Configuration form elements
  const configSonioxKey = document.querySelector("#config-soniox-key") as HTMLInputElement;
  const configSonioxFormat = document.querySelector("#config-soniox-format") as HTMLSelectElement;
  const configSonioxEnable = document.querySelector("#config-soniox-enable") as HTMLInputElement;
  const configOpenaiKey = document.querySelector("#config-openai-key") as HTMLInputElement;
  const configOpenaiModel = document.querySelector("#config-openai-model") as HTMLSelectElement;
  const configOpenaiGateModel = document.querySelector("#config-openai-gate-model") as HTMLSelectElement;
  const configOpenaiEnable = document.querySelector("#config-openai-enable") as HTMLInputElement;
  const configRecordingFormat = document.querySelector("#config-recording-format") as HTMLSelectElement;
  const configRecordingQuality = document.querySelector("#config-recording-quality") as HTMLSelectElement;
  const configRecordingAuto = document.querySelector("#config-recording-auto") as HTMLInputElement;
  const configDefaultAssistant = document.querySelector("#config-default-assistant") as HTMLSelectElement;

  // Fetch models for configuration modal
  async function fetchModelsForConfig() {
    const apiKey = configOpenaiKey?.value.trim();
    if (!apiKey || !configOpenaiModel || !configOpenaiGateModel) {
      return;
    }

    try {
      const models = await invoke<string[]>("get_openai_models", {
        apiKey: apiKey
      });

      // Save current selections
      const currentModel = configOpenaiModel.value;
      const currentGateModel = configOpenaiGateModel.value;

      // Clear and repopulate main model dropdown
      configOpenaiModel.innerHTML = "";
      models.forEach(model => {
        const option = document.createElement("option");
        option.value = model;
        option.textContent = model;
        configOpenaiModel.appendChild(option);
      });

      // Clear and repopulate gate model dropdown
      configOpenaiGateModel.innerHTML = "";
      models.forEach(model => {
        const option = document.createElement("option");
        option.value = model;
        option.textContent = model;
        configOpenaiGateModel.appendChild(option);
      });

      // Restore selections if they exist in the fetched models
      if (models.includes(currentModel)) {
        configOpenaiModel.value = currentModel;
      } else if (models.length > 0) {
        configOpenaiModel.value = models[0];
      }

      if (models.includes(currentGateModel)) {
        configOpenaiGateModel.value = currentGateModel;
      } else if (models.includes("gpt-4o-mini")) {
        configOpenaiGateModel.value = "gpt-4o-mini";
      } else if (models.length > 0) {
        configOpenaiGateModel.value = models[0];
      }

      console.log(`✅ Loaded ${models.length} OpenAI models for config`);
    } catch (error) {
      console.error("❌ Failed to fetch models for config:", error);
      // Keep default options if fetching fails
    }
  }

  // Load configuration from backend
  async function loadConfiguration() {
    try {
      const config = await invoke<any>("load_app_config");

      // Populate form with loaded config (non-model fields first)
      if (configSonioxKey) configSonioxKey.value = config.soniox.api_key || "";
      if (configSonioxFormat) configSonioxFormat.value = config.soniox.audio_format || "pcm_s16le";
      if (configSonioxEnable) configSonioxEnable.checked = config.ui.enable_soniox || false;

      if (configOpenaiKey) configOpenaiKey.value = config.openai.api_key || "";
      if (configOpenaiEnable) configOpenaiEnable.checked = config.ui.enable_openai || false;

      if (configRecordingFormat) configRecordingFormat.value = config.recording.default_format || "mp3";
      if (configRecordingQuality) configRecordingQuality.value = config.recording.default_quality || "verylow";
      if (configRecordingAuto) configRecordingAuto.checked = config.recording.auto_detect_enabled !== false;

      if (configDefaultAssistant) configDefaultAssistant.value = config.ui.default_assistant || "general";

      // Fetch models first, then set configured values
      if (configOpenaiKey?.value.trim()) {
        await fetchModelsForConfig();

        // After fetching models, set the configured model values
        if (configOpenaiModel && config.openai.model) {
          configOpenaiModel.value = config.openai.model;
        }
        if (configOpenaiGateModel && config.openai.gate_model) {
          configOpenaiGateModel.value = config.openai.gate_model;
        }
      } else {
        // No API key, set placeholder values for models
        if (configOpenaiModel) configOpenaiModel.value = config.openai.model || "";
        if (configOpenaiGateModel) configOpenaiGateModel.value = config.openai.gate_model || "";
      }

      console.log("✅ Configuration loaded successfully");
    } catch (error) {
      console.error("❌ Failed to load configuration:", error);
      // Still allow the modal to open with default values
    }
  }

  // Save configuration to backend
  async function saveConfiguration() {
    try {
      const config = {
        soniox: {
          api_key: configSonioxKey?.value || "",
          audio_format: configSonioxFormat?.value || "pcm_s16le",
          translation: "none"
        },
        openai: {
          api_key: configOpenaiKey?.value || "",
          model: configOpenaiModel?.value || "gpt-4.1",
          gate_model: configOpenaiGateModel?.value || "gpt-4.1-nano"
        },
        recording: {
          default_format: configRecordingFormat?.value || "mp3",
          default_quality: configRecordingQuality?.value || "verylow",
          auto_detect_enabled: configRecordingAuto?.checked !== false
        },
        ui: {
          enable_soniox: configSonioxEnable?.checked || false,
          enable_openai: configOpenaiEnable?.checked || false,
          default_assistant: configDefaultAssistant?.value || "general"
        }
      };

      await invoke("save_app_config", { config });
      console.log("✅ Configuration saved successfully");

      // Update UI with saved values
      updateUIFromConfig(config);

      return true;
    } catch (error) {
      console.error("❌ Failed to save configuration:", error);
      alert(`Failed to save configuration: ${error}`);
      return false;
    }
  }

  // Update main UI elements with configuration values
  function updateUIFromConfig(config: any) {
    // Update main UI form elements to reflect saved configuration
    if (sonioxApiKeyInp) sonioxApiKeyInp.value = config.soniox.api_key || "";
    if (sonioxEnableChk) sonioxEnableChk.checked = config.ui.enable_soniox || false;
    if (openaiApiKeyInp) openaiApiKeyInp.value = config.openai.api_key || "";
    if (openaiModelSel) openaiModelSel.value = config.openai.model || "gpt-4.1";
    if (openaiEnableChk) openaiEnableChk.checked = config.ui.enable_openai || false;
    if (formatSel) formatSel.value = config.recording.default_format || "mp3";
    if (qualitySel) qualitySel.value = config.recording.default_quality || "verylow";
    if (autoDetectChk) autoDetectChk.checked = config.recording.auto_detect_enabled !== false;
    if (assistantSel) assistantSel.value = config.ui.default_assistant || "general";

    // Trigger change events to update dependent UI states
    formatSel?.dispatchEvent(new Event("change"));
  }

  // Populate assistant selector in config modal
  async function populateConfigAssistants() {
    try {
      const assistants = await invoke<any[]>("get_assistants");
      if (configDefaultAssistant && assistants.length > 0) {
        configDefaultAssistant.innerHTML = "";
        assistants.forEach(assistant => {
          const option = document.createElement("option");
          option.value = assistant.id;
          option.textContent = assistant.name;
          configDefaultAssistant.appendChild(option);
        });
      }
    } catch (error) {
      console.error("❌ Failed to populate config assistants:", error);
    }
  }

  // Show modal
  configBtn?.addEventListener("click", async () => {
    await loadConfiguration();
    await populateConfigAssistants();
    configModal?.classList.remove("hidden");
  });

  // Hide modal
  const hideModal = () => {
    configModal?.classList.add("hidden");
  };

  configClose?.addEventListener("click", hideModal);
  configCancel?.addEventListener("click", hideModal);

  // Close modal on backdrop click
  configModal?.addEventListener("click", (e) => {
    if (e.target === configModal) {
      hideModal();
    }
  });

  // Save and close modal
  configSave?.addEventListener("click", async () => {
    const success = await saveConfiguration();
    if (success) {
      hideModal();
    }
  });

  // Fetch models when API key changes in config modal
  configOpenaiKey?.addEventListener("input", () => {
    // Debounce the API call
    clearTimeout((window as any).configModelTimeout);
    (window as any).configModelTimeout = setTimeout(async () => {
      if (configOpenaiKey?.value.trim().length) {
        await fetchModelsForConfig();
      }
    }, 1000);
  });
}

window.addEventListener("DOMContentLoaded", async () => {
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
  assistantSel = document.querySelector("#assistant-select");
  openaiModelSel = document.querySelector("#openai-model");
  openaiEnableChk = document.querySelector("#openai-enable");
  openaiStatusEl = document.querySelector("#openai-status");
  aiAnalysisEl = document.querySelector("#ai-analysis");

  // Panel toggle functionality
  setupPanelToggles();

  // Setup configuration modal
  setupConfigModal();

  // Load configuration on startup
  await loadInitialConfiguration();

  // Load assistants on startup
  await loadAssistants();

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
  gateCountEl = document.getElementById("gate-count");
  aiPrevBtn = document.getElementById("ai-prev") as HTMLButtonElement | null;
  aiNextBtn = document.getElementById("ai-next") as HTMLButtonElement | null;
  aiPosEl = document.getElementById("ai-pos");

  // History navigation
  aiPrevBtn?.addEventListener("click", () => {
    if (aiIndex > 0) {
      aiIndex -= 1;
      renderAiAnswer();
    }
  });
  aiNextBtn?.addEventListener("click", () => {
    if (aiIndex >= 0 && aiIndex < aiAnswers.length - 1) {
      aiIndex += 1;
      renderAiAnswer();
    }
  });
  listen<string>("soniox-transcript", (event) => {
    console.log("📝 Received soniox-transcript event:", event.payload);

    if (transcriptEl) {
      // Remove placeholder if present
      const placeholder = transcriptEl.querySelector(".placeholder");
      if (placeholder) {
        placeholder.remove();
      }

      transcriptEl.textContent = event.payload;
      transcriptEl.scrollTop = transcriptEl.scrollHeight;
      console.log("✅ Updated transcript element with:", event.payload);
    } else {
      console.error("❌ Transcript element not found!");
    }

    // Store transcript for OpenAI analysis
    lastTranscript = event.payload;

    // Heuristic gating: only analyze when transcript is stable and meaningfully changed
    if (openaiEnableChk?.checked) {
      const stable = stripTentative(lastTranscript).trim();
      const now = Date.now();
      const delta = stable.length - lastAnalyzedStable.length;

      const prevStable = lastAnalyzedStable;
      if (
        stable.length > 50 &&
        isStable(stable) &&
        delta >= MIN_NEW_CHARS &&
        now - lastAnalysisAt >= MIN_ANALYSIS_INTERVAL_MS &&
        !analyzing
      ) {
        console.log(`🔎 Running gated analysis (len=${stable.length}, Δ=${delta})`);

        // Run the lightweight gate for observability only (does not block or decide)
        if (openaiApiKeyInp?.value.trim()) {
          const key = openaiApiKeyInp.value.trim();
          const lastOut = aiAnalysisEl?.textContent || '';
          // Fire and forget; update counter when done
          invoke<any>("should_run_analysis_gate", {
            apiKey: key,
            model: null, // Let backend use configured gate_model
            assistantId: assistantSel?.value || null,
            currentTranscript: stable,
            previousTranscript: prevStable,
            lastOutput: lastOut,
          }).then((res) => {
            gateRuns += 1;
            if (gateCountEl) gateCountEl.textContent = `Gate: ${gateRuns}`;
            console.log("Gate decision:", res);
          }).catch((err) => {
            gateRuns += 1; // still count attempted runs
            if (gateCountEl) gateCountEl.textContent = `Gate: ${gateRuns}`;
            console.warn("Gate error:", err);
          });
        }

        lastAnalyzedStable = stable;
        lastAnalysisAt = now;
        analyzeWithOpenAI(stable);
      }
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

// Push a new AI answer into history and show it
function pushAiAnswer(answer: string) {
  aiAnswers.push(answer);
  aiIndex = aiAnswers.length - 1; // auto-move to most recent
  renderAiAnswer();
}

// Render current AI answer and update navigation state
function renderAiAnswer() {
  if (!aiAnalysisEl) return;

  // Manage placeholder
  const placeholder = aiAnalysisEl.querySelector(".placeholder");
  if (aiAnswers.length === 0) {
    if (!placeholder) {
      const ph = document.createElement("div");
      ph.className = "placeholder";
      ph.textContent = "No answers yet.";
      aiAnalysisEl.innerHTML = "";
      aiAnalysisEl.appendChild(ph);
    }
  } else {
    if (placeholder) placeholder.remove();
    aiAnalysisEl.textContent = aiAnswers[aiIndex] || "";
    aiAnalysisEl.scrollTop = aiAnalysisEl.scrollHeight;
  }

  // Update nav controls
  if (aiPrevBtn) aiPrevBtn.disabled = !(aiIndex > 0);
  if (aiNextBtn) aiNextBtn.disabled = !(aiIndex >= 0 && aiIndex < aiAnswers.length - 1);
  if (aiPosEl) aiPosEl.textContent = `${Math.max(0, aiIndex + 1)}/${aiAnswers.length}`;
}
