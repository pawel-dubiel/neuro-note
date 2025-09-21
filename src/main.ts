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
let openrouterApiKeyInp: HTMLInputElement | null;
let assistantSel: HTMLSelectElement | null;
let aiProviderSel: HTMLSelectElement | null;
let aiModelSel: HTMLSelectElement | null;
let aiEnableChk: HTMLInputElement | null;
let aiStatusEl: HTMLElement | null;
let openrouterCreditsEl: HTMLElement | null;
let aiAnalysisEl: HTMLElement | null;
let aiPrevBtn: HTMLButtonElement | null;
let aiNextBtn: HTMLButtonElement | null;
let aiPosEl: HTMLElement | null;
let btnClearSession: HTMLButtonElement | null;
let aiAnswers: string[] = [];
let aiIndex: number = -1; // -1 means no history yet
let lastTranscript = "";
let lastAnalyzedStable = "";
let lastAnalysisAt = 0;
let gateCountEl: HTMLElement | null;
let gateRuns = 0;
let modelCountEl: HTMLElement | null;
let modelRuns = 0;
let gateLastEl: HTMLElement | null;
let transcriptEl: HTMLElement | null;
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

const providerSelectedModels: Record<string, string> = {
  openai: "gpt-4.1",
  openrouter: "deepseek/deepseek-chat-v3-0324:free",
};

const providerModelCache: Record<string, string[]> = {
  openai: [],
  openrouter: [],
};

function getCurrentProvider(): "openai" | "openrouter" {
  return aiProviderSel?.value === "openrouter" ? "openrouter" : "openai";
}

function getApiKeyForProvider(provider: "openai" | "openrouter"): string {
  if (provider === "openrouter") {
    return openrouterApiKeyInp?.value.trim() || "";
  }
  return openaiApiKeyInp?.value.trim() || "";
}

async function fetchModelsForMain(provider: "openai" | "openrouter") {
  const apiKey = getApiKeyForProvider(provider);
  if (!apiKey) {
    providerModelCache[provider] = [];
    if (getCurrentProvider() === provider) {
      populateModelOptions(provider);
    }
    return;
  }

  try {
    const models = await invoke<string[]>("get_ai_models", { provider, apiKey });
    providerModelCache[provider] = models;

    if (!providerSelectedModels[provider] || !models.includes(providerSelectedModels[provider])) {
      providerSelectedModels[provider] = models[0] || "";
    }

    if (getCurrentProvider() === provider) {
      populateModelOptions(provider);
    }
  } catch (error) {
    console.error(`❌ Failed to fetch ${provider} models:`, error);
  }
}

function populateModelOptions(provider: "openai" | "openrouter") {
  const select = aiModelSel;
  if (!select) return;

  const models = providerModelCache[provider] || [];
  select.innerHTML = "";

  if (models.length === 0) {
    const option = document.createElement("option");
    option.value = "";
    option.textContent = "Enter API key to load models...";
    select.appendChild(option);
    select.disabled = true;
    return;
  }

  select.disabled = false;
  models.forEach((model) => {
    const option = document.createElement("option");
    option.value = model;
    option.textContent = model;
    select.appendChild(option);
  });

  const desired = providerSelectedModels[provider];
  if (desired && models.includes(desired)) {
    select.value = desired;
  } else {
    select.value = models[0];
    providerSelectedModels[provider] = models[0];
  }
}

async function refreshOpenrouterCredits() {
  if (!openrouterCreditsEl) return;

  if (getCurrentProvider() !== "openrouter") {
    updateOpenrouterCreditsDisplay(null);
    return;
  }

  const apiKey = getApiKeyForProvider("openrouter");
  if (!apiKey) {
    updateOpenrouterCreditsDisplay(null);
    return;
  }

  updateOpenrouterCreditsDisplay(undefined);

  try {
    const summary = await invoke<{ total_credits: number; total_usage: number }>("get_openrouter_credits", { apiKey });
    updateOpenrouterCreditsDisplay(summary);
  } catch (error) {
    console.error("❌ Failed to load OpenRouter credits:", error);
    if (openrouterCreditsEl) {
      openrouterCreditsEl.style.display = "";
      openrouterCreditsEl.textContent = "Credits: ?";
      openrouterCreditsEl.setAttribute("title", `Failed to load credits: ${error}`);
    }
  }
}

function updateOpenrouterCreditsDisplay(summary: { total_credits: number; total_usage: number } | null | undefined) {
  if (!openrouterCreditsEl) return;

  if (getCurrentProvider() !== "openrouter") {
    openrouterCreditsEl.style.display = "none";
    openrouterCreditsEl.textContent = "";
    openrouterCreditsEl.removeAttribute("title");
    return;
  }

  openrouterCreditsEl.style.display = "";

  if (summary === undefined) {
    openrouterCreditsEl.textContent = "Credits: …";
    openrouterCreditsEl.removeAttribute("title");
    return;
  }

  if (summary === null) {
    const apiKey = getApiKeyForProvider("openrouter");
    openrouterCreditsEl.textContent = apiKey ? "Credits: --" : "";
    if (!apiKey) openrouterCreditsEl.style.display = "none";
    openrouterCreditsEl.removeAttribute("title");
    return;
  }

  const remaining = Math.max(summary.total_credits - summary.total_usage, 0);
  openrouterCreditsEl.textContent = `Credits: ${remaining.toFixed(2)}`;
  openrouterCreditsEl.setAttribute(
    "title",
    `Total: ${summary.total_credits.toFixed(2)} • Used: ${summary.total_usage.toFixed(2)}`,
  );
}

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
    applyConfigToUi(config);

    console.log("✅ Initial configuration loaded successfully");
  } catch (error) {
    console.log("ℹ️ No configuration found, using defaults:", error);
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

// AI analysis function
let analyzing = false;
let currentStreamId: string | null = null;
let currentStreamText = "";

async function analyzeWithAI(transcriptOverride?: string) {
  const transcriptToAnalyze = (transcriptOverride ?? lastTranscript).trim();
  const provider = getCurrentProvider();
  const apiKey = getApiKeyForProvider(provider);

  if (analyzing || !apiKey || !transcriptToAnalyze) {
    return;
  }

  analyzing = true;
  const requestId = typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.round(Math.random() * 1e6)}`;

  currentStreamId = requestId;
  currentStreamText = "";

  setAiStatus("analyzing", "Analyzing...");

  if (aiAnalysisEl) {
    const placeholder = aiAnalysisEl.querySelector(".placeholder");
    if (placeholder) placeholder.remove();
    aiAnalysisEl.textContent = "";
  }

  const selectedModel = aiModelSel?.value || providerSelectedModels[provider];
  const lastAnswer = aiAnswers.length > 0 ? aiAnswers[aiAnswers.length - 1] : null;

  modelRuns += 1;
  if (modelCountEl) modelCountEl.textContent = `Model: ${modelRuns}`;

  try {
    await invoke("stream_ai_analysis", {
      provider,
      apiKey,
      model: selectedModel || null,
      assistantId: assistantSel?.value || null,
      requestId,
      transcript: transcriptToAnalyze,
      lastOutput: lastAnswer,
    });
  } catch (error) {
    console.error("❌ AI analysis error:", error);
    handleAiStreamError(requestId, String(error));
  }
}

type AiStatusState = "ready" | "analyzing" | "error";

function setAiStatus(state: AiStatusState, text: string) {
  if (!aiStatusEl) return;
  aiStatusEl.textContent = text;
  aiStatusEl.classList.remove("ready", "analyzing", "error");
  aiStatusEl.classList.add(state);
}

type AiStreamPayload = {
  request_id: string;
  segment?: string | null;
  final_text?: string | null;
  done: boolean;
};

function processAiStreamPayload(payload: AiStreamPayload) {
  if (!currentStreamId || payload.request_id !== currentStreamId) {
    return;
  }

  if (payload.segment) {
    currentStreamText += payload.segment;
    if (aiAnalysisEl) {
      const placeholder = aiAnalysisEl.querySelector(".placeholder");
      if (placeholder) placeholder.remove();
      aiAnalysisEl.textContent = currentStreamText;
      aiAnalysisEl.scrollTop = aiAnalysisEl.scrollHeight;
    }
  }

  if (payload.done) {
    analyzing = false;
    currentStreamId = null;

    const finalText = (payload.final_text ?? currentStreamText).trim();
    currentStreamText = finalText;

    setAiStatus("ready", "Ready");

    if (aiAnalysisEl) {
      const placeholder = aiAnalysisEl.querySelector(".placeholder");
      if (placeholder) placeholder.remove();
      aiAnalysisEl.textContent = finalText;
      aiAnalysisEl.scrollTop = aiAnalysisEl.scrollHeight;
    }

    if (finalText.length > 0) {
      pushAiAnswer(finalText);
    }
  }
}

function handleAiStreamError(requestId: string, message: string) {
  if (currentStreamId && currentStreamId !== requestId) {
    return;
  }

  analyzing = false;
  currentStreamId = null;
  currentStreamText = "";
  setAiStatus("error", "Error");

  if (aiAnalysisEl) {
    aiAnalysisEl.textContent = `Error: ${message}`;
  }
}

async function clearTranscriptAndHistory() {
  lastTranscript = "";
  lastAnalyzedStable = "";
  lastAnalysisAt = 0;
  analyzing = false;
  currentStreamId = null;
  currentStreamText = "";

  if (transcriptEl) {
    transcriptEl.innerHTML = `<div class="placeholder">${DEFAULT_TRANSCRIPT_PLACEHOLDER}</div>`;
    transcriptEl.scrollTop = 0;
  }

  aiAnswers = [];
  aiIndex = -1;
  renderAiAnswer();
  setAiStatus("ready", "Ready");

  if (aiPrevBtn) aiPrevBtn.disabled = true;
  if (aiNextBtn) aiNextBtn.disabled = true;
  if (aiPosEl) aiPosEl.textContent = "0/0";

  gateRuns = 0;
  if (gateCountEl) gateCountEl.textContent = "Gate: 0";
  modelRuns = 0;
  if (modelCountEl) modelCountEl.textContent = "Model: 0";
  if (gateLastEl) {
    gateLastEl.textContent = "Decision: -";
    gateLastEl.removeAttribute("title");
  }

  try {
    await invoke("clear_transcript_state");
  } catch (err) {
    console.error("Failed to clear backend transcript state:", err);
  }

  console.log("🧹 Cleared transcript, AI history, and counters.");
}

// Heuristic gating helpers
const DEFAULT_TRANSCRIPT_PLACEHOLDER = "Start recording to see live transcript...";
const DEFAULT_AI_PLACEHOLDER = "Enable AI analysis to see insights and follow-up questions...";
const MIN_ANALYSIS_INTERVAL_MS = 4000; // throttle expensive calls
const MIN_NEW_CHARS = 30;              // require meaningful delta

function stripTentative(text: string): string {
  // Remove tentative segments (underscored) and Soniox separator line
  return text
    .replace(/_+[^_]*_+/g, "")
    .replace(/\n=+\s*$/g, "");
}

function isStable(text: string): boolean {
  // Consider stable if it ends with sentence punctuation and has no visible tentative segment
  const ends = /[.!?)]\s*$/.test(text.trim());
  const hasTentative = /_+[^_]*_+/.test(text);
  return ends && !hasTentative;
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

  const configSonioxKey = document.querySelector("#config-soniox-key") as HTMLInputElement;
  const configSonioxFormat = document.querySelector("#config-soniox-format") as HTMLSelectElement;
  const configSonioxEnable = document.querySelector("#config-soniox-enable") as HTMLInputElement;

  const configAiProvider = document.querySelector("#config-ai-provider") as HTMLSelectElement;
  const configAiEnable = document.querySelector("#config-ai-enable") as HTMLInputElement;

  const configOpenaiKey = document.querySelector("#config-openai-key") as HTMLInputElement;
  const configOpenaiModel = document.querySelector("#config-openai-model") as HTMLSelectElement;
  const configOpenaiGateModel = document.querySelector("#config-openai-gate-model") as HTMLSelectElement;

  const configOpenrouterKey = document.querySelector("#config-openrouter-key") as HTMLInputElement;
  const configOpenrouterModel = document.querySelector("#config-openrouter-model") as HTMLSelectElement;
  const configOpenrouterGateModel = document.querySelector("#config-openrouter-gate-model") as HTMLSelectElement;
  const configOpenrouterCredits = document.querySelector("#config-openrouter-credits") as HTMLElement;

  const configRecordingFormat = document.querySelector("#config-recording-format") as HTMLSelectElement;
  const configRecordingQuality = document.querySelector("#config-recording-quality") as HTMLSelectElement;
  const configRecordingAuto = document.querySelector("#config-recording-auto") as HTMLInputElement;
  const configDefaultAssistant = document.querySelector("#config-default-assistant") as HTMLSelectElement;

  let latestConfig: any = null;
  let openaiDebounce: number | undefined;
  let openrouterDebounce: number | undefined;

  const fillSelect = (
    select: HTMLSelectElement | null,
    models: string[],
    desired: string | undefined,
    fallback: string
  ) => {
    if (!select) return;
    select.innerHTML = "";
    if (models.length === 0) {
      const option = document.createElement("option");
      option.value = "";
      option.textContent = "Enter API key to load models...";
      select.appendChild(option);
      select.disabled = true;
      return;
    }

    select.disabled = false;
    models.forEach((model) => {
      const option = document.createElement("option");
      option.value = model;
      option.textContent = model;
      select.appendChild(option);
    });

    if (desired && models.includes(desired)) {
      select.value = desired;
    } else if (models.includes(fallback)) {
      select.value = fallback;
    } else {
      select.value = models[0];
    }
  };

  async function fetchConfigModels(provider: "openai" | "openrouter") {
    const key = provider === "openrouter"
      ? configOpenrouterKey?.value.trim()
      : configOpenaiKey?.value.trim();
    const modelSelect = provider === "openrouter" ? configOpenrouterModel : configOpenaiModel;
    const gateSelect = provider === "openrouter" ? configOpenrouterGateModel : configOpenaiGateModel;
    if (!key || !modelSelect || !gateSelect) return;

    try {
      const models = await invoke<string[]>("get_ai_models", { provider, apiKey: key });
      if (provider === "openrouter") {
        fillSelect(modelSelect, models, latestConfig?.openrouter?.model, "deepseek/deepseek-chat-v3-0324:free");
        fillSelect(gateSelect, models, latestConfig?.openrouter?.gate_model, "deepseek/deepseek-chat-v3-0324:free");
      } else {
        fillSelect(modelSelect, models, latestConfig?.openai?.model, "gpt-4.1");
        fillSelect(gateSelect, models, latestConfig?.openai?.gate_model, "gpt-4.1-nano");
      }
    } catch (error) {
      console.error(`❌ Failed to fetch ${provider} models for config:`, error);
    }
  }

  async function refreshConfigOpenrouterCredits() {
    if (!configOpenrouterCredits) return;
    const key = configOpenrouterKey?.value.trim();
    if (!key) {
      configOpenrouterCredits.textContent = "--";
      configOpenrouterCredits.removeAttribute("title");
      return;
    }

    configOpenrouterCredits.textContent = "…";
    configOpenrouterCredits.removeAttribute("title");
    try {
      const summary = await invoke<{ total_credits: number; total_usage: number }>("get_openrouter_credits", { apiKey: key });
      const remaining = Math.max(summary.total_credits - summary.total_usage, 0);
      configOpenrouterCredits.textContent = `${remaining.toFixed(2)} (used ${summary.total_usage.toFixed(2)})`;
      configOpenrouterCredits.setAttribute(
        "title",
        `Total: ${summary.total_credits.toFixed(2)} • Used: ${summary.total_usage.toFixed(2)}`,
      );
    } catch (error) {
      configOpenrouterCredits.textContent = "?";
      configOpenrouterCredits.setAttribute("title", `Failed to load credits: ${error}`);
      console.error("❌ Failed to load OpenRouter credits for config:", error);
    }
  }

  async function loadConfiguration() {
    try {
      const config = await invoke<any>("load_app_config");
      latestConfig = config;

      if (configSonioxKey) configSonioxKey.value = config.soniox?.api_key || "";
      if (configSonioxFormat) configSonioxFormat.value = config.soniox?.audio_format || "pcm_s16le";
      if (configSonioxEnable) configSonioxEnable.checked = config.ui?.enable_soniox || false;

      if (configAiProvider) configAiProvider.value = config.ui?.ai_provider || "openai";
      if (configAiEnable) configAiEnable.checked = config.ui?.enable_ai || false;

      if (configOpenaiKey) configOpenaiKey.value = config.openai?.api_key || "";
      if (configOpenrouterKey) configOpenrouterKey.value = config.openrouter?.api_key || "";

      fillSelect(configOpenaiModel, config.openai?.model ? [config.openai.model] : [], config.openai?.model, "gpt-4.1");
      fillSelect(configOpenaiGateModel, config.openai?.gate_model ? [config.openai.gate_model] : [], config.openai?.gate_model, "gpt-4.1-nano");
      fillSelect(configOpenrouterModel, config.openrouter?.model ? [config.openrouter.model] : [], config.openrouter?.model, "deepseek/deepseek-chat-v3-0324:free");
      fillSelect(configOpenrouterGateModel, config.openrouter?.gate_model ? [config.openrouter.gate_model] : [], config.openrouter?.gate_model, "deepseek/deepseek-chat-v3-0324:free");

      if (configRecordingFormat) configRecordingFormat.value = config.recording?.default_format || "mp3";
      if (configRecordingQuality) configRecordingQuality.value = config.recording?.default_quality || "verylow";
      if (configRecordingAuto) configRecordingAuto.checked = config.recording?.auto_detect_enabled !== false;
      if (configDefaultAssistant) configDefaultAssistant.value = config.ui?.default_assistant || "general";

      if (configOpenaiKey?.value.trim()) {
        await fetchConfigModels("openai");
      }
      if (configOpenrouterKey?.value.trim()) {
        await fetchConfigModels("openrouter");
        await refreshConfigOpenrouterCredits();
      } else if (configOpenrouterCredits) {
        configOpenrouterCredits.textContent = "--";
        configOpenrouterCredits.removeAttribute("title");
      }
    } catch (error) {
      console.error("❌ Failed to load configuration:", error);
      alert(`Failed to load configuration: ${error}`);
    }
  }

  async function saveConfiguration() {
    try {
      const config = {
        soniox: {
          api_key: configSonioxKey?.value || "",
          audio_format: configSonioxFormat?.value || "pcm_s16le",
          translation: "none",
        },
        openai: {
          api_key: configOpenaiKey?.value || "",
          model: configOpenaiModel?.value || "gpt-4.1",
          gate_model: configOpenaiGateModel?.value || "gpt-4.1-nano",
        },
        openrouter: {
          api_key: configOpenrouterKey?.value || "",
          model: configOpenrouterModel?.value || "deepseek/deepseek-chat-v3-0324:free",
          gate_model: configOpenrouterGateModel?.value || "deepseek/deepseek-chat-v3-0324:free",
        },
        recording: {
          default_format: configRecordingFormat?.value || "mp3",
          default_quality: configRecordingQuality?.value || "verylow",
          auto_detect_enabled: configRecordingAuto?.checked !== false,
        },
        ui: {
          enable_soniox: configSonioxEnable?.checked || false,
          enable_ai: configAiEnable?.checked || false,
          default_assistant: configDefaultAssistant?.value || "general",
          ai_provider: configAiProvider?.value || "openai",
        },
      };

      await invoke("save_app_config", { config });
      console.log("✅ Configuration saved successfully");

      latestConfig = config;
      applyConfigToUi(config);

      return true;
    } catch (error) {
      console.error("❌ Failed to save configuration:", error);
      alert(`Failed to save configuration: ${error}`);
      return false;
    }
  }

  async function populateConfigAssistants() {
    try {
      const assistants = await invoke<any[]>("get_assistants");
      if (configDefaultAssistant && assistants.length > 0) {
        configDefaultAssistant.innerHTML = "";
        assistants.forEach((assistant) => {
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

  const hideModal = () => {
    configModal?.classList.add("hidden");
  };

  configBtn?.addEventListener("click", async () => {
    await loadConfiguration();
    await populateConfigAssistants();
    configModal?.classList.remove("hidden");
  });

  configClose?.addEventListener("click", hideModal);
  configCancel?.addEventListener("click", hideModal);

  configModal?.addEventListener("click", (e) => {
    if (e.target === configModal) {
      hideModal();
    }
  });

  configSave?.addEventListener("click", async () => {
    const success = await saveConfiguration();
    if (success) {
      hideModal();
    }
  });

  configOpenaiKey?.addEventListener("input", () => {
    window.clearTimeout(openaiDebounce);
    openaiDebounce = window.setTimeout(() => {
      if (configOpenaiKey?.value.trim()) {
        fetchConfigModels("openai");
      }
    }, 800);
  });

  configOpenrouterKey?.addEventListener("input", () => {
    window.clearTimeout(openrouterDebounce);
    openrouterDebounce = window.setTimeout(() => {
      if (configOpenrouterKey?.value.trim()) {
        fetchConfigModels("openrouter");
        refreshConfigOpenrouterCredits();
      } else if (configOpenrouterCredits) {
        configOpenrouterCredits.textContent = "--";
        configOpenrouterCredits.removeAttribute("title");
      }
    }, 800);
  });
}

function applyConfigToUi(config: any) {
  if (sonioxApiKeyInp) sonioxApiKeyInp.value = config.soniox?.api_key || "";
  if (sonioxEnableChk) sonioxEnableChk.checked = config.ui?.enable_soniox || false;

  if (openaiApiKeyInp) openaiApiKeyInp.value = config.openai?.api_key || "";
  if (openrouterApiKeyInp) openrouterApiKeyInp.value = config.openrouter?.api_key || "";

  providerSelectedModels.openai = config.openai?.model || providerSelectedModels.openai;
  providerSelectedModels.openrouter = config.openrouter?.model || providerSelectedModels.openrouter;

  if (aiProviderSel) aiProviderSel.value = config.ui?.ai_provider || "openai";
  if (aiEnableChk) aiEnableChk.checked = config.ui?.enable_ai || false;
  if (assistantSel) assistantSel.value = config.ui?.default_assistant || "general";

  if (formatSel) formatSel.value = config.recording?.default_format || "mp3";
  if (qualitySel) qualitySel.value = config.recording?.default_quality || "verylow";
  if (autoDetectChk) autoDetectChk.checked = config.recording?.auto_detect_enabled !== false;

  formatSel?.dispatchEvent(new Event("change"));

  if (config.openai?.api_key) {
    fetchModelsForMain("openai");
  } else {
    providerModelCache.openai = [];
    if (getCurrentProvider() === "openai") {
      populateModelOptions("openai");
    }
  }

  if (config.openrouter?.api_key) {
    fetchModelsForMain("openrouter");
  } else {
    providerModelCache.openrouter = [];
    if (getCurrentProvider() === "openrouter") {
      populateModelOptions("openrouter");
    }
  }

  updateOpenrouterCreditsDisplay(null);
  populateModelOptions(getCurrentProvider());
  refreshOpenrouterCredits();
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
  openrouterApiKeyInp = document.querySelector("#openrouter-api");
  assistantSel = document.querySelector("#assistant-select");
  aiProviderSel = document.querySelector("#ai-provider");
  aiModelSel = document.querySelector("#ai-model");
  aiEnableChk = document.querySelector("#ai-enable");
  aiStatusEl = document.querySelector("#ai-status");
  openrouterCreditsEl = document.querySelector("#openrouter-credits");
  aiAnalysisEl = document.querySelector("#ai-analysis");
  transcriptEl = document.querySelector("#transcript");
  btnClearSession = document.querySelector("#btn-clear-session") as HTMLButtonElement | null;

  // Panel toggle functionality
  setupPanelToggles();

  // Setup configuration modal
  setupConfigModal();

  aiProviderSel?.addEventListener("change", async () => {
    const provider = getCurrentProvider();
    if (getApiKeyForProvider(provider)) {
      await fetchModelsForMain(provider);
    } else {
      populateModelOptions(provider);
    }
    await refreshOpenrouterCredits();

    if (aiEnableChk?.checked && lastTranscript.trim().length > 20) {
      analyzeWithAI();
    }
  });

  aiModelSel?.addEventListener("change", () => {
    const provider = getCurrentProvider();
    if (aiModelSel?.value) {
      providerSelectedModels[provider] = aiModelSel.value;
    }
  });

  // Load configuration on startup
  await loadInitialConfiguration();

  // Load assistants on startup
  await loadAssistants();

  btnClearSession?.addEventListener("click", () => {
    void clearTranscriptAndHistory();
  });

  btnStart?.addEventListener("click", start);
  btnPause?.addEventListener("click", pause);
  btnResume?.addEventListener("click", resume);
  btnStop?.addEventListener("click", stop);

  // OpenAI enable checkbox listener
  aiEnableChk?.addEventListener("change", () => {
    if (aiEnableChk?.checked && lastTranscript.trim().length > 20) {
      analyzeWithAI();
    }
  });

  // OpenAI API key input listener - fetch models when key is pasted/entered
  openaiApiKeyInp?.addEventListener("input", () => {
    // Debounce the API call
    clearTimeout((window as any).openaiModelTimeout);
    (window as any).openaiModelTimeout = setTimeout(() => {
      if (openaiApiKeyInp?.value.trim().length) {
        fetchModelsForMain("openai");
      } else {
        providerModelCache.openai = [];
        if (getCurrentProvider() === "openai") {
          populateModelOptions("openai");
        }
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

  listen<AiStreamPayload>("ai-analysis-stream", (event) => {
    processAiStreamPayload(event.payload);
  });

  listen<{ request_id: string; message: string }>("ai-analysis-error", (event) => {
    handleAiStreamError(event.payload.request_id, event.payload.message);
  });

  // Soniox transcript events
  gateCountEl = document.getElementById("gate-count");
  modelCountEl = document.getElementById("model-count");
  gateLastEl = document.getElementById("gate-last");
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

    // Store transcript for AI analysis
    lastTranscript = event.payload;

    // Heuristic gating: only analyze when transcript is stable and meaningfully changed
    if (aiEnableChk?.checked) {
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

        // Use lightweight gate to decide whether to invoke main model (strict)
        const provider = getCurrentProvider();
        const key = getApiKeyForProvider(provider);
        if (key) {
          const lastOut = aiAnalysisEl?.textContent || '';
          // Count a real gate request before invoking
          gateRuns += 1;
          if (gateCountEl) gateCountEl.textContent = `Gate: ${gateRuns}`;

          invoke<{ run: boolean; instruction?: string; reason?: string; confidence?: number }>("should_run_analysis_gate", {
            provider,
            apiKey: key,
            model: null, // Let backend use configured gate_model
            assistantId: assistantSel?.value || null,
            currentTranscript: stable,
            previousTranscript: prevStable,
            lastOutput: lastOut,
          }).then((res) => {
            console.log("Gate decision:", res);
            if (gateLastEl) {
              const decision = res?.instruction || (res?.run ? "NEEDED" : "NOT_NEEDED");
              const conf = typeof res?.confidence === 'number' ? ` (${(res.confidence * 100).toFixed(0)}%)` : '';
              gateLastEl.textContent = `Decision: ${decision}`;
              gateLastEl.setAttribute('title', res?.reason ? `${res.reason}${conf}` : `Gate decision${conf}`);
            }
            if (res?.run) {
              lastAnalyzedStable = stable;
              lastAnalysisAt = now;
              analyzeWithAI(stable);
            } else {
              console.log("⏭️ Skipping analysis due to gate decision.");
            }
          }).catch((err) => {
            console.warn("Gate error:", err);
            if (gateLastEl) {
              gateLastEl.textContent = `Decision: ERROR`;
              gateLastEl.setAttribute('title', String(err));
            }
            // Strict mode: do not run analysis on gate error
          });
        } else {
          // Strict mode: no gate key => do not analyze
          console.warn("Gate strict: missing AI provider API key; skipping analysis.");
        }
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
      ph.textContent = DEFAULT_AI_PLACEHOLDER;
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
