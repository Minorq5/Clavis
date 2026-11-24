<script lang="ts">
  import { onMount, tick } from "svelte";
  import { api, type Settings, type Stage } from "../lib/api";
  import { t, i18n } from "../lib/i18n.svelte";
  import Wordmark from "../lib/Wordmark.svelte";
  import HotkeyInput from "../lib/HotkeyInput.svelte";
  import Icon from "../lib/Icon.svelte";

  let { stage, ondone }: { stage: Stage; ondone: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let step = $state(stage === "setup" ? 2 : 1);
  let code = $state("");
  let codeErr = $state("");
  let attempts = $state(15);
  let busy = $state(false);
  let requesting = $state(false);
  let reqName = $state("");
  let reqMsg = $state("");
  let reqSent = $state(false);
  let reqErr = $state("");
  let pin = $state("");
  let pin2 = $state("");
  let pinErr = $state("");
  let words = $state<string[]>([]);
  let wrote = $state(false);
  let copiedWords = $state(false);
  let settings = $state<Settings | null>(null);

  onMount(async () => {
    const st = await api.status();
    attempts = st.attemptsLeft;
    settings = await api.settings();
    await tick();
    document.querySelector<HTMLInputElement>("input")?.focus();
  });

  // Код показывается группами по 4: XXXX-XXXX-…
  function formatCode(v: string) {
    const raw = v.toUpperCase().replace(/[^A-Z0-9]/g, "").slice(0, 24);
    return raw.replace(/(.{4})(?=.)/g, "$1-");
  }

  async function activate(e: Event) {
    e.preventDefault();
    busy = true;
    codeErr = "";
    try {
      const r = await api.activate(code);
      attempts = r.attemptsLeft;
      if (r.ok) { step = 2; await tick(); document.querySelector<HTMLInputElement>("#pin")?.focus(); }
      else if (r.error === "network") codeErr = t("setup.netError");
      else if (r.error === "clock") codeErr = t("setup.clockError");
      else if (r.error) codeErr = t("setup.serverError");
      else if (r.attemptsLeft <= 0) { codeErr = t("setup.blockedNow"); setTimeout(() => ondone(), 1500); }
      else codeErr = t("setup.codeWrong", { n: r.attemptsLeft });
    } finally {
      busy = false;
    }
  }

  async function sendRequest(e: Event) {
    e.preventDefault();
    try {
      await api.requestAccess(reqName.trim(), reqMsg.trim());
      reqSent = true;
      reqErr = "";
    } catch (err) {
      reqErr = String(err) === "network" ? t("setup.netError") : t("setup.serverError");
    }
  }

  async function savePin(e: Event) {
    e.preventDefault();
    if (pin !== pin2) { pinErr = t("setup.pinMismatch"); return; }
    busy = true;
    try {
      words = await api.setPin(pin);
      pin = pin2 = "";
      step = 3;
    } catch (err) {
      pinErr = String(err);
    } finally {
      busy = false;
    }
  }

  async function copyWords() {
    await api.copyText(words.join(" "));
    copiedWords = true;
  }

  async function finishPhrase() {
    await api.confirmRecovery();
    step = 4;
  }

  async function finish() {
    if (settings) await api.settingsSave($state.snapshot(settings));
    ondone();
  }

  const steps = ["setup.step1", "setup.step2", "setup.step3", "setup.step4"] as const;
  const pinOk = $derived(/^\d{4,12}$/.test(pin) && pin2.length > 0);
</script>

<div class="setup">
  <aside>
    <Wordmark size={17} />
    <ol>
      {#each steps as s, i}
        <li class:done={step > i + 1} class:now={step === i + 1}><span class="n">{step > i + 1 ? "✓" : i + 1}</span>{t(s)}</li>
      {/each}
    </ol>
    <div class="lang">
      <button class:on={i18n.lang === "ru"} onclick={() => { i18n.lang = "ru"; if (settings) settings.lang = "ru"; }}>RU</button>
      <button class:on={i18n.lang === "en"} onclick={() => { i18n.lang = "en"; if (settings) settings.lang = "en"; }}>EN</button>
    </div>
  </aside>

  <main>
    {#if step === 1 && !requesting}
      <p class="welcome">{t("setup.welcome")}</p>
      <form onsubmit={activate}>
        <label for="code">{t("setup.codeLabel")}</label>
        <input id="code" class="field code" value={code} oninput={(e) => (code = formatCode(e.currentTarget.value))} placeholder="XXXX-XXXX-XXXX-XXXX-XXXX-XXXX" spellcheck="false" autocomplete="off" />
        <p class="help">{t("setup.codeHelp")}</p>
        {#if codeErr}<p class="err">{codeErr}</p>{/if}
        {#if attempts < 15}<p class="warn">{t("setup.codeWarn")}</p>{/if}
        <div class="actions">
          <button type="button" class="btn ghost" onclick={() => (requesting = true)}>{t("setup.noCode")}</button>
          <button class="btn primary" disabled={code.length !== 29 || busy}>{t("setup.codeGo")}</button>
        </div>
      </form>
    {:else if step === 1}
      {#if reqSent}
        <p class="welcome">{t("setup.reqSent")}</p>
        <div class="actions"><button class="btn" onclick={() => { requesting = false; reqSent = false; }}>{t("setup.back")}</button></div>
      {:else}
        <form onsubmit={sendRequest}>
          <label for="rn">{t("setup.reqName")}</label>
          <input id="rn" class="field" bind:value={reqName} maxlength="40" />
          <label for="rm">{t("setup.reqMsg")}</label>
          <textarea id="rm" class="field" rows="3" bind:value={reqMsg} maxlength="280"></textarea>
          {#if reqErr}<p class="err">{reqErr}</p>{/if}
          <div class="actions">
            <button type="button" class="btn ghost" onclick={() => (requesting = false)}>{t("setup.back")}</button>
            <button class="btn primary" disabled={!reqName.trim()}>{t("setup.reqSend")}</button>
          </div>
        </form>
      {/if}
    {:else if step === 2}
      <form onsubmit={savePin}>
        <label for="pin">{t("setup.pinLabel")}</label>
        <input id="pin" class="field pin" type="password" inputmode="numeric" maxlength="12" bind:value={pin} oninput={() => (pinErr = "")} />
        <p class="help">{t("setup.pinHelp")}</p>
        <label for="pin2">{t("setup.pinRepeat")}</label>
        <input id="pin2" class="field pin" type="password" inputmode="numeric" maxlength="12" bind:value={pin2} oninput={() => (pinErr = "")} />
        {#if pinErr}<p class="err">{pinErr}</p>{/if}
        <div class="actions"><button class="btn primary" disabled={!pinOk || busy}>{t("setup.next")}</button></div>
      </form>
    {:else if step === 3}
      <p class="help wide">{t("setup.phraseHelp")}</p>
      <ol class="words">
        {#each words as w, i}<li><span>{i + 1}</span>{w}</li>{/each}
      </ol>
      <label class="confirm"><input type="checkbox" bind:checked={wrote} />{t("setup.phraseCheck")}</label>
      <div class="actions" style="max-width:540px">
        <button class="btn ghost" onclick={copyWords}><Icon name={copiedWords ? "check" : "copy"} />{copiedWords ? t("common.copied") : t("setup.phraseCopy")}</button>
        <button class="btn primary" disabled={!wrote} onclick={finishPhrase}>{t("setup.next")}</button>
      </div>
    {:else if settings}
      <p class="help wide">{t("setup.ctrlHelp")}</p>
      <div class="ctrl">
        <span>{t("set.hotkey")}</span>
        <HotkeyInput bind:value={settings.hotkey} />
      </div>
      <div class="ctrl">
        <span>{t("set.mode")}</span>
        <div class="choices">
          <button class="choice" class:on={settings.mode === "popup"} onclick={() => (settings!.mode = "popup")}>
            <svg viewBox="0 0 120 72"><rect x="1" y="1" width="118" height="70" rx="4" /><rect class="fill" x="38" y="20" width="56" height="38" rx="2" /><path d="M34 16l3 9 2.5-3.5L44 21z" /></svg>
            {t("set.mode.popup")}
          </button>
          <button class="choice" class:on={settings.mode === "panel"} onclick={() => (settings!.mode = "panel")}>
            <svg viewBox="0 0 120 72"><rect x="1" y="1" width="118" height="70" rx="4" /><rect class="fill" x="84" y="1" width="35" height="70" /></svg>
            {t("set.mode.panel")}
          </button>
        </div>
      </div>
      <div class="ctrl">
        <span>{t("set.expander")}</span>
        <label class="confirm inline"><input type="checkbox" bind:checked={settings.expander.enabled} />{t("set.expanderOn")}</label>
      </div>
      <div class="actions"><button class="btn primary" onclick={finish}>{t("setup.done")}</button></div>
    {/if}
  </main>
</div>

<style>
  .setup { display: grid; grid-template-columns: 220px 1fr; height: 100%; }
  aside { border-right: 1px solid var(--line); padding: 26px 22px; display: flex; flex-direction: column; gap: 32px; }
  ol { list-style: none; margin: 0; padding: 0; display: grid; gap: 14px; }
  aside li { display: flex; align-items: center; gap: 10px; color: var(--faint); font-size: var(--t-s); }
  aside li.now { color: var(--text); }
  aside li.done { color: var(--muted); }
  .n { width: 20px; height: 20px; border-radius: 50%; border: 1px solid currentColor; display: grid; place-items: center; font-size: 10.5px; flex: none; }
  li.now .n { background: var(--text); color: var(--bg); border-color: var(--text); }
  .lang { margin-top: auto; display: flex; gap: 4px; }
  .lang button { font-size: var(--t-xs); color: var(--faint); padding: 3px 6px; border-radius: var(--r-s); }
  .lang button.on { color: var(--text); box-shadow: inset 0 0 0 1px var(--line-strong); }

  main { padding: 64px 56px 40px; overflow-y: auto; }
  main > *, form > * { max-width: 460px; }
  form { display: grid; gap: 8px; }
  label { font-size: var(--t-s); color: var(--muted); margin-top: 8px; }
  .welcome { font: 400 var(--t-xl)/1.45 var(--font-display); margin: 0 0 32px; max-width: 30ch; }
  .code { font: 500 15px/1 var(--mono); letter-spacing: 0.05em; padding: 11px 12px; }
  .pin { font-size: 18px; letter-spacing: 0.3em; width: 220px; }
  .help { color: var(--faint); font-size: var(--t-s); margin: 0; line-height: 1.55; }
  .help.wide { color: var(--muted); font-size: var(--t-m); margin-bottom: 20px; max-width: 54ch; }
  .err { color: var(--danger); margin: 4px 0 0; font-size: var(--t-s); }
  .warn { color: var(--muted); margin: 0; font-size: var(--t-s); }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 22px; }

  .words { grid-template-columns: repeat(4, 1fr); gap: 6px 18px; padding: 16px 18px; border: 1px solid var(--line); border-radius: var(--r-m); background: var(--sunk); max-width: 540px; }
  .words li { display: flex; gap: 8px; font: 500 13px/1.7 var(--mono); user-select: text; }
  .words span { color: var(--faint); width: 18px; text-align: right; font-weight: 400; }
  .confirm { display: flex; align-items: center; gap: 10px; color: var(--text); font-size: var(--t-m); margin-top: 20px; cursor: pointer; }
  .confirm.inline { margin: 0; }
  .confirm input { appearance: none; width: 16px; height: 16px; border: 1px solid var(--muted); border-radius: 3px; margin: 0; display: grid; place-items: center; cursor: pointer; }
  .confirm input:checked { background: var(--text); border-color: var(--text); }
  .confirm input:checked::after { content: ""; width: 8px; height: 4px; border: solid var(--bg); border-width: 0 0 1.6px 1.6px; transform: rotate(-45deg) translate(1px, -1px); }

  .ctrl { display: grid; gap: 8px; margin-bottom: 22px; }
  .ctrl > span { font-size: var(--t-s); color: var(--muted); }
  .choices { display: flex; gap: 10px; }
  .choice { display: grid; gap: 8px; padding: 10px; border: 1px solid var(--line); border-radius: var(--r-m); font-size: var(--t-s); color: var(--muted); text-align: left; }
  .choice svg { width: 150px; height: 90px; fill: none; stroke: var(--line-strong); }
  .choice .fill { fill: var(--raise); stroke: var(--muted); }
  .choice path { fill: var(--muted); stroke: none; }
  .choice.on { border-color: var(--text); color: var(--text); }
  .choice.on .fill { fill: var(--text); stroke: var(--text); }
</style>
