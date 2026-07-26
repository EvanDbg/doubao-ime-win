import "./style.css";
import "./backend.css";

type VoiceState = "idle" | "recording" | "processing";
type Config = {
  general: { auto_start: boolean; language: string };
  hotkey: { action: string; binding: string; mode: string; combo_key: string; double_tap_key: string; double_tap_interval: number; raw_vk_code: number; raw_scan_code: number; raw_extended: boolean };
  floating_button: { enabled: boolean; position_x: number; position_y: number };
  asr: { vad_enabled: boolean; aec_enabled: boolean; audio_quality: string; punctuation_mode: string; end_smooth_window_ms: number; post_ratio_gain: number };
  cloud: { ner_enabled: boolean; auto_polish_enabled: boolean; llm_context_enabled: boolean; llm_custom_api_enabled?: boolean | null; llm_base_url: string; llm_api_key: string; llm_model: string; llm_prompt: string; llm_thinking_mode: string; llm_reasoning_effort: string };
};

declare global { interface Window { __doubaoEvent?: (event: Record<string, unknown>) => void; ipc?: { postMessage(message: string): void } } }

const app = document.querySelector<HTMLDivElement>("#app")!;
const isHud = new URLSearchParams(location.search).get("view") === "hud";
let config: Config | null = null;
let state: VoiceState = "idle";
let meter = 0;
let activeSettingsPage = "general";
const post = (command: string, params: Record<string, unknown> = {}) => window.ipc?.postMessage(JSON.stringify({ command, params }));
const esc = (value: unknown) => String(value ?? "").replace(/[&<>'"]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" }[c]!));
const field = (id: string, label: string, value: unknown, type = "text") => `<label class="field"><span>${label}</span><input id="${id}" type="${type}" value="${esc(value)}" /></label>`;
const toggle = (id: string, label: string, on: boolean, hint = "") => `<label class="toggle"><span><b>${label}</b>${hint && `<small>${hint}</small>`}</span><input id="${id}" type="checkbox" ${on ? "checked" : ""}/><i></i></label>`;
const select = (id: string, label: string, value: string, options: [string, string][]) => `<label class="field"><span>${label}</span><select id="${id}">${options.map(([v, text]) => `<option value="${v}" ${v === value ? "selected" : ""}>${text}</option>`).join("")}</select></label>`;

function showSettingsPage(page: string) {
  const button = Array.from(document.querySelectorAll<HTMLButtonElement>("nav [data-page]"))
    .find(node => node.dataset.page === page);
  const section = document.getElementById(page);
  if (!button || !section?.classList.contains("settings-page")) return;
  activeSettingsPage = page;
  document.querySelectorAll("nav [data-page], .settings-page").forEach(node => node.classList.remove("active"));
  button.classList.add("active");
  section.classList.add("active");
  const title = document.querySelector<HTMLElement>("#page-title");
  if (title) title.textContent = button.textContent?.trim() || "设置";
}

function renderSettings() {
  if (!config) { app.innerHTML = `<main class="loading">正在加载设置...</main>`; return; }
  const c = config;
  const hotkeyType = c.hotkey.binding === "raw" ? "raw" : "standard";
  const triggerMode = ["single_tap", "double_tap", "hold"].includes(c.hotkey.mode) ? c.hotkey.mode : "single_tap";
  const standardKey = c.hotkey.mode === "combo" ? c.hotkey.combo_key : c.hotkey.double_tap_key;
  const customApiEnabled = c.cloud.llm_custom_api_enabled ?? Boolean(c.cloud.llm_base_url.trim());
  app.innerHTML = `<div class="shell"><div class="window-titlebar" data-window-drag><div class="window-title"><strong>D</strong><span>豆包语音输入</span></div><div class="window-controls"><button id="minimize" title="最小化" aria-label="最小化">&#8212;</button><button id="maximize" title="最大化" aria-label="最大化">&#9633;</button><button class="close" id="close" title="关闭" aria-label="关闭">&#10005;</button></div></div><aside><div class="brand"><strong>D</strong><div><b>豆包语音输入</b></div></div><nav aria-label="设置分类"><button class="active" data-page="general">常规设置</button><button data-page="hotkeys">热键配置</button><button data-page="floating">悬浮窗口</button><button data-page="asr">识别引擎</button><button data-page="cloud">云端增强</button></nav></aside><main class="settings-main"><header><h1 id="page-title">常规设置</h1><div class="header-actions"><button class="primary" id="save">保存更改</button></div></header><div class="page-stack">
  <section class="settings-page active" id="general"><h2>常规设置</h2><div class="grid">${toggle("auto_start", "开机自动启动", c.general.auto_start, "登录 Windows 后启动服务")}</div><div class="row"><button class="secondary" id="open-logs">打开日志文件夹</button><span>按日期保存运行日志，便于排查问题</span></div></section>
  <section class="settings-page" id="hotkeys"><h2>热键配置</h2><div class="grid two">${select("hotkey_action", "触发动作", c.hotkey.action || "voice_input", [["voice_input", "本程序语音输入"], ["official_hold", "官方豆包：长按（右 Alt）"], ["official_hands_free", "官方豆包：免按（左 Ctrl + Win）"]])}<div data-hotkey-action="voice_input">${select("hotkey_type", "按键类型", hotkeyType, [["standard", "标准按键"], ["raw", "非标准按键"]])}</div><div data-hotkey-action="voice_input">${select("trigger_mode", "触发类型", triggerMode, [["single_tap", "单击"], ["double_tap", "双击"], ["hold", "长按"]])}</div><div data-hotkey-action="voice_input" data-hotkey-type="standard">${field("standard_key", "触发按键", standardKey)}</div><div data-hotkey-action="voice_input" data-trigger-mode="double_tap">${field("double_tap_interval", "双击间隔（毫秒）", c.hotkey.double_tap_interval, "number")}</div></div><div class="row" data-hotkey-type="raw"><button class="secondary" id="capture">录入非标准按键</button><span id="capture-status">${c.hotkey.binding === "raw" ? `已录入：键码 ${c.hotkey.raw_vk_code} / 扫描码 ${c.hotkey.raw_scan_code}` : "可录入小爱键、媒体键等厂商按键"}</span></div><div class="row" data-hotkey-action="official_hold"><span>按住自定义按键说话，松开结束。触发时转发官方右 Alt 快捷键，本程序不录音、不弹出悬浮窗；该按键将由本程序独占。</span></div><div class="row" data-hotkey-action="official_hands_free"><span>按一下开始说话，再按一下结束。触发时转发官方左 Ctrl + Win 快捷键，本程序不录音、不弹出悬浮窗；该按键将由本程序独占。</span></div></section>
  <section class="settings-page" id="floating"><h2>悬浮窗口</h2><div class="grid two">${toggle("floating_enabled", "启用录音悬浮窗口", c.floating_button.enabled, "录音时显示置顶状态窗口")}<div class="preview"><b>●</b><span><strong>正在聆听</strong><small>实时音频电平</small></span><em></em></div></div></section>
  <section class="settings-page" id="asr"><h2>识别引擎</h2><div class="grid two">${toggle("vad", "本地语音活动检测", c.asr.vad_enabled)}${toggle("aec", "回声消除（实验性）", c.asr.aec_enabled)}${select("audio_quality", "音频质量", c.asr.audio_quality, [["standard", "标准 16 千赫兹"], ["high_quality", "高质量 24 千赫兹"]])}${select("punctuation", "标点模式", c.asr.punctuation_mode, [["smart", "智能标点"], ["spaces", "空格分词"], ["no_sentence_final", "无句末标点"], ["preserve", "保留服务端结果"]])}${field("smooth", "尾音平滑（毫秒）", c.asr.end_smooth_window_ms, "number")}${field("gain", "麦克风增益", c.asr.post_ratio_gain, "number")}</div></section>
  <section class="settings-page" id="cloud"><h2>云端增强</h2><div class="grid two">${toggle("ner", "实体识别", c.cloud.ner_enabled)}${toggle("polish", "LLM 润色", c.cloud.auto_polish_enabled, "关闭后保留 ASR 原文，不发送润色请求")}</div><div class="polish-settings" id="polish-settings" ${c.cloud.auto_polish_enabled ? "" : "hidden"}><div class="grid two">${toggle("context", "读取光标上下文", c.cloud.llm_context_enabled, "仅用于润色请求")}${toggle("custom_api", "使用自定义接口", customApiEnabled, "关闭时使用项目内置的豆包润色服务")}</div><div class="backend-note" id="backend-note"></div><div class="custom-api-fields" id="custom-api-fields" ${customApiEnabled ? "" : "hidden"}><div class="grid two">${field("llm_url", "兼容接口地址", c.cloud.llm_base_url)}${field("llm_model", "模型名称", c.cloud.llm_model)}${field("llm_key", "接口密钥", c.cloud.llm_api_key, "password")}${select("thinking", "深度思考", c.cloud.llm_thinking_mode, [["omit", "不发送参数"], ["disabled", "关闭"], ["enabled", "开启"]])}${select("reasoning", "推理强度", c.cloud.llm_reasoning_effort, [["", "不发送参数"], ["low", "低"], ["medium", "中"], ["high", "高"]])}</div><div class="api-test"><button class="secondary" id="test-llm">测试连接</button><span id="llm-test-result" role="status"></span></div></div><label class="field full"><span>润色提示词（留空使用内置规则）</span><textarea id="llm_prompt">${esc(c.cloud.llm_prompt)}</textarea></label></div></section></div></main></div>`;
  document.querySelector("#save")?.addEventListener("click", save);
  document.querySelector("#close")?.addEventListener("click", () => post("hide_settings"));
  document.querySelector("#minimize")?.addEventListener("click", () => post("minimize_settings"));
  document.querySelector("#maximize")?.addEventListener("click", () => post("toggle_settings_maximize"));
  document.querySelectorAll<HTMLButtonElement>("nav [data-page]").forEach(button => button.addEventListener("click", () => {
    const page = button.dataset.page;
    if (page) showSettingsPage(page);
  }));
  document.querySelectorAll<HTMLElement>("[data-window-drag]").forEach(node => node.addEventListener("mousedown", event => {
    if (event.button !== 0 || event.detail > 1 || (event.target as HTMLElement).closest("button, input, select, textarea, label")) return;
    post("drag_settings");
  }));
  document.querySelector(".window-titlebar")?.addEventListener("dblclick", event => {
    if (!(event.target as HTMLElement).closest("button")) post("toggle_settings_maximize");
  });
  document.querySelector("#capture")?.addEventListener("click", () => { post("capture_raw_key"); setCapture("请在 10 秒内按下要绑定的按键..."); });
  document.querySelector("#hotkey_action")?.addEventListener("change", syncHotkeyFields);
  document.querySelector("#hotkey_type")?.addEventListener("change", syncHotkeyFields);
  document.querySelector("#trigger_mode")?.addEventListener("change", syncHotkeyFields);
  syncHotkeyFields();
  document.querySelector("#open-logs")?.addEventListener("click", () => post("open_logs"));
  const polishToggle = document.querySelector<HTMLInputElement>("#polish");
  const customApiToggle = document.querySelector<HTMLInputElement>("#custom_api");
  const updateBackendMode = () => {
    const custom = Boolean(customApiToggle?.checked);
    const fields = document.querySelector<HTMLElement>("#custom-api-fields");
    if (fields) fields.hidden = !custom;
    const note = document.querySelector<HTMLElement>("#backend-note");
    if (note) note.innerHTML = custom
      ? `<b>自定义兼容接口</b><small>请求将发送到下方地址，并使用你提供的密钥和模型。</small>`
      : `<b>内置豆包润色</b><small>使用已注册的本机设备凭据调用内置服务，无需接口密钥。</small>`;
  };
  const updatePolishMode = () => {
    const settings = document.querySelector<HTMLElement>("#polish-settings");
    if (settings) settings.hidden = !polishToggle?.checked;
    updateBackendMode();
  };
  polishToggle?.addEventListener("change", updatePolishMode);
  customApiToggle?.addEventListener("change", updateBackendMode);
  document.querySelector("#test-llm")?.addEventListener("click", testCustomLlm);
  updatePolishMode();
  showSettingsPage(activeSettingsPage);
  post("get_settings_window_state");
}
function setCapture(message: string) { const node = document.querySelector("#capture-status"); if (node) node.textContent = message; }
function syncHotkeyFields() {
  const action = document.querySelector<HTMLSelectElement>("#hotkey_action")?.value || "voice_input";
  const typeSelect = document.querySelector<HTMLSelectElement>("#hotkey_type");
  if (action !== "voice_input" && typeSelect) typeSelect.value = "raw";
  const type = typeSelect?.value;
  const mode = document.querySelector<HTMLSelectElement>("#trigger_mode")?.value;
  document.querySelectorAll<HTMLElement>("[data-hotkey-action], [data-hotkey-type], [data-trigger-mode]").forEach(node => {
    const actionMatches = !node.dataset.hotkeyAction || node.dataset.hotkeyAction === action;
    const typeMatches = !node.dataset.hotkeyType || node.dataset.hotkeyType === type;
    const modeMatches = !node.dataset.triggerMode || node.dataset.triggerMode === mode;
    node.hidden = !(actionMatches && typeMatches && modeMatches);
  });
}
function value(id: string) { return document.querySelector<HTMLInputElement | HTMLSelectElement>(`#${id}`)!.value; }
function enabled(id: string) { return document.querySelector<HTMLInputElement>(`#${id}`)!.checked; }
function formCloudConfig(): Config["cloud"] {
  if (!config) throw new Error("设置尚未加载");
  return {
    ...config.cloud,
    ner_enabled: enabled("ner"),
    auto_polish_enabled: enabled("polish"),
    llm_context_enabled: enabled("context"),
    llm_custom_api_enabled: enabled("custom_api"),
    llm_base_url: value("llm_url"),
    llm_model: value("llm_model"),
    llm_api_key: value("llm_key"),
    llm_thinking_mode: value("thinking"),
    llm_reasoning_effort: value("reasoning"),
    llm_prompt: document.querySelector<HTMLTextAreaElement>("#llm_prompt")!.value,
  };
}
function testCustomLlm() {
  if (!config) return;
  const button = document.querySelector<HTMLButtonElement>("#test-llm");
  const result = document.querySelector<HTMLElement>("#llm-test-result");
  if (button) button.disabled = true;
  if (result) { result.textContent = "正在测试连接..."; result.className = "testing"; }
  post("test_custom_llm", { config: formCloudConfig() });
}
function save() {
  if (!config) return;
  const hotkeyAction = value("hotkey_action");
  Object.assign(config.general, { auto_start: enabled("auto_start"), language: "zh-CN" });
  Object.assign(config.hotkey, { action: hotkeyAction, binding: hotkeyAction === "voice_input" ? value("hotkey_type") : "raw", double_tap_key: value("standard_key"), double_tap_interval: Number(value("double_tap_interval")), mode: hotkeyAction === "official_hold" ? "hold" : hotkeyAction === "official_hands_free" ? "single_tap" : value("trigger_mode") });
  Object.assign(config.floating_button, { enabled: enabled("floating_enabled") });
  Object.assign(config.asr, { vad_enabled: enabled("vad"), aec_enabled: enabled("aec"), audio_quality: value("audio_quality"), punctuation_mode: value("punctuation"), end_smooth_window_ms: Number(value("smooth")), post_ratio_gain: Number(value("gain")) });
  Object.assign(config.cloud, formCloudConfig());
  post("save_config", { config });
}
function renderHud() {
  const recording = state === "recording";
  app.innerHTML = `<div class="hud ${state}" id="hud"><div class="hud-top"><b><i>${recording ? "●" : "◌"}</i>${recording ? "正在聆听" : "正在处理"}</b></div><div class="wave">${Array.from({ length: 18 }, () => "<i></i>").join("")}</div></div>`;
  document.querySelector<HTMLElement>("#hud")?.addEventListener("mousedown", event => { if (event.button === 0) post("drag_hud"); });
  paintMeter();
}
function paintMeter() { document.querySelectorAll<HTMLElement>(".wave i").forEach((bar, index) => { const curve = 0.5 + Math.abs(8.5 - index) / 15; bar.style.height = `${5 + meter * 34 * curve}px`; }); }
window.__doubaoEvent = event => { if (event.type === "config") { config = event.config as Config; renderSettings(); } if (event.type === "voice_state") { state = event.state as VoiceState; if (isHud) renderHud(); } if (event.type === "window_state") { const button = document.querySelector<HTMLButtonElement>("#maximize"); const maximized = Boolean(event.maximized); if (button) { button.innerHTML = maximized ? "&#10064;" : "&#9633;"; button.title = maximized ? "还原" : "最大化"; button.setAttribute("aria-label", button.title); } } if (event.type === "meter") { meter = Number(event.value) || 0; if (isHud) paintMeter(); } if (event.type === "capture_result") { const binding = event.binding as { vk_code: number; scan_code: number; extended: boolean } | undefined; if (binding && config) { Object.assign(config.hotkey, { binding: "raw", raw_vk_code: binding.vk_code, raw_scan_code: binding.scan_code, raw_extended: binding.extended }); const kind = document.querySelector<HTMLSelectElement>("#hotkey_type"); if (kind) kind.value = "raw"; syncHotkeyFields(); } setCapture(String(event.message)); } if (event.type === "llm_test_result") { const button = document.querySelector<HTMLButtonElement>("#test-llm"); const result = document.querySelector<HTMLElement>("#llm-test-result"); if (button) button.disabled = false; if (result) { result.textContent = String(event.message); result.className = event.success ? "success" : "failure"; } } if (event.type === "error") alert(String(event.message)); };
if (isHud) { renderHud(); post("get_voice_state"); } else { renderSettings(); post("get_config"); }
