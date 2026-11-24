<script lang="ts">
  import { onMount } from "svelte";
  import { api, inTauri, type JournalEntry, type Settings } from "../lib/api";
  import { ask } from "../lib/dialog.svelte";
  import { applyTheme } from "../lib/theme";
  import { updates, checkUpdates, installUpdate } from "../lib/updates.svelte";
  import { VERSION } from "../lib/version";
  import Modal from "../lib/Modal.svelte";
  import LegalView from "../lib/LegalView.svelte";
  import type { LegalDoc } from "../lib/legal";
  import { t, i18n } from "../lib/i18n.svelte";
  import HotkeyInput from "../lib/HotkeyInput.svelte";
  import Icon from "../lib/Icon.svelte";
  import { toast } from "../lib/toast.svelte";

  let s = $state<Settings | null>(null);
  let paused = $state(false);
  let excludedText = $state("");
  let cleared = $state(false);

  let hello = $state<{ available: boolean; enabled: boolean }>({ available: false, enabled: false });
  let helloBusy = $state(false);
  let consents = $state<string[]>([]);
  let journalOpen = $state(false);
  let journal = $state<JournalEntry[]>([]);
  let checkedNow = $state(false);
  let legalOpen = $state<LegalDoc | null>(null);
  let eraseOpen = $state(false);
  let eraseWord = $state("");
  let eraseErr = $state("");
  let erasing = $state(false);
  let isOwner = $state(false);

  async function erase(e: Event) {
    e.preventDefault();
    if (eraseWord.trim().toUpperCase() !== t("set.eraseWord")) return;
    erasing = true;
    eraseErr = "";
    try {
      await api.eraseEverything();
    } catch (err) {
      const m = String(err);
      eraseErr = m === "owner" ? t("set.eraseOwner") : m === "network" ? t("qa.network") : t("toast.error", { e: m });
    } finally {
      erasing = false;
    }
  }

  onMount(async () => {
    [s, paused] = await Promise.all([api.settings(), api.historyPaused()]);
    excludedText = s.excludedApps.join("\n");
    hello = await api.helloStatus().catch(() => hello);
    consents = await api.consents().catch(() => []);
    isOwner = (await api.status().catch(() => null))?.owner ?? false;
  });

  async function toggleHello() {
    helloBusy = true;
    try {
      if (hello.enabled) await api.helloDisable();
      else await api.helloEnable();
      hello = await api.helloStatus();
    } catch (e) {
      const m = String(e);
      if (m !== "cancelled") toast(m === "hello_unstable" ? t("set.helloUnstable") : m === "unavailable" ? t("set.helloUnavailable") : t("toast.error", { e: m }), 4000);
    } finally {
      helloBusy = false;
    }
  }

  async function toggleConsent(service: "translate" | "shorten") {
    if (consents.includes(service)) await api.consentRevoke(service);
    else await api.consentGive(service);
    consents = await api.consents();
  }

  async function openJournal() {
    journal = await api.journal().catch(() => []);
    journalOpen = true;
  }

  async function check() {
    checkedNow = false;
    await checkUpdates();
    checkedNow = true;
  }

  const when = (at: number) => new Date(at).toLocaleString(i18n.lang === "ru" ? "ru-RU" : "en-GB", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
  const jKey = (k: string) => ("j." + k) as Parameters<typeof t>[0];

  let hotkeyErr = $state("");
  let quickErr = $state("");
  let backupOpen = $state<"export" | "import" | null>(null);
  let backupPw = $state("");
  let backupErr = $state("");
  let backupBusy = $state(false);

  async function runBackup(e: Event) {
    e.preventDefault();
    if (backupPw.length < 8) return (backupErr = t("set.backupShort"));
    backupBusy = true;
    backupErr = "";
    try {
      if (backupOpen === "export") {
        const p = await api.backupExport(backupPw);
        if (p) { toast(t("set.backupDone", { p }), 4000); backupOpen = null; }
      } else {
        if (!(await ask({ title: t("set.backupRestoreQ"), text: t("set.backupRestoreText"), ok: t("set.backupRestore"), danger: true }))) return;
        if (await api.backupImport(backupPw)) { toast(t("set.backupRestored"), 4000); backupOpen = null; }
      }
    } catch (err) {
      const m = String(err);
      backupErr = m === "wrong_password" || m === "format" ? t("set.backupWrong") : m === "password_short" ? t("set.backupShort") : t("toast.error", { e: m });
    } finally {
      backupBusy = false;
      backupPw = backupOpen ? backupPw : "";
    }
  }
  async function save() {
    if (!s) return;
    s.excludedApps = excludedText.split("\n").map((x) => x.trim()).filter(Boolean);
    i18n.lang = s.lang;
    try {
      await api.settingsSave($state.snapshot(s));
      hotkeyErr = "";
      quickErr = "";
      applyTheme(s.theme);
    } catch (e) {
      if (String(e) === "hotkey" || String(e) === "quick_hotkey") {
        if (String(e) === "hotkey") hotkeyErr = t("set.hotkeyBusy");
        else quickErr = t("set.quickBusy");
        s = await api.settings();
      } else toast(t("toast.error", { e: String(e) }));
    }
  }

  async function exportSnippets() {
    try {
      const p = await api.snippetsExport();
      if (p) toast(t("toast.exported", { p }), 3500);
    } catch (e) {
      toast(t("toast.error", { e: String(e) }));
    }
  }
  async function importSnippets() {
    try {
      const n = await api.snippetsImport();
      if (n !== null) toast(t("toast.imported", { n }));
    } catch (e) {
      toast(t("toast.error", { e: String(e) }));
    }
  }
</script>

{#if s}
  <div class="page">
    <section>
      <h2>{t("set.general")}</h2>
      <div class="row">
        <div class="lbl"><span>{t("set.hotkey")}</span><small class:bad={hotkeyErr}>{hotkeyErr || t("set.hotkeyHelp")}</small></div>
        <HotkeyInput bind:value={s.hotkey} onchange={save} />
      </div>
      <div class="row">
        <div class="lbl"><span>{t("set.mode")}</span></div>
        <div class="seg" role="radiogroup">
          <button role="radio" aria-checked={s.mode === "popup"} onclick={() => { s!.mode = "popup"; save(); }}>{t("set.mode.popup")}</button>
          <button role="radio" aria-checked={s.mode === "panel"} onclick={() => { s!.mode = "panel"; save(); }}>{t("set.mode.panel")}</button>
        </div>
      </div>
      <div class="row">
        <div class="lbl"><span>{t("set.enter")}</span><small>{t("set.enterHelp")}</small></div>
        <div class="seg" role="radiogroup">
          <button role="radio" aria-checked={s.enterAction === "paste"} onclick={() => { s!.enterAction = "paste"; save(); }}>{t("set.enter.paste")}</button>
          <button role="radio" aria-checked={s.enterAction === "copy"} onclick={() => { s!.enterAction = "copy"; save(); }}>{t("set.enter.copy")}</button>
        </div>
      </div>
      <div class="row">
        <div class="lbl"><span>{t("set.limit")}</span></div>
        <select class="field narrow" bind:value={s.historyLimit} onchange={save}>
          {#each [50, 100, 200, 500] as n}<option value={n}>{t("set.limitN", { n })}</option>{/each}
        </select>
      </div>
      <div class="row">
        <div class="lbl"><span>{t("set.displayName")}</span><small>{t("set.displayNameHelp")}</small></div>
        <input class="field narrow" maxlength="40" bind:value={s.displayName} onchange={save} placeholder={t("set.namePh")} />
      </div>
      <div class="row">
        <div class="lbl"><span>{t("set.lang")}</span></div>
        <div class="seg" role="radiogroup">
          <button role="radio" aria-checked={s.lang === "ru"} onclick={() => { s!.lang = "ru"; save(); }}>Русский</button>
          <button role="radio" aria-checked={s.lang === "en"} onclick={() => { s!.lang = "en"; save(); }}>English</button>
        </div>
      </div>
      <label class="row check">
        <div class="lbl"><span>{t("set.autostart")}</span></div>
        <input type="checkbox" class="switch" bind:checked={s.autostart} onchange={save} />
      </label>
    </section>

    <section>
      <h2>{t("set.expander")}</h2>
      <div class="row">
        <div class="lbl"><span>{t("set.quickHotkey")}</span><small class:bad={quickErr}>{quickErr || t("set.quickHotkeyHelp")}</small></div>
        <HotkeyInput bind:value={s.quickHotkey} onchange={save} />
      </div>
      <div class="row">
        <div class="lbl"><span>{t("set.theme")}</span></div>
        <div class="seg" role="radiogroup" aria-label={t("set.theme")}>
          {#each [["dark", "set.themeDark"], ["light", "set.themeLight"], ["system", "set.themeSystem"]] as [v, k]}
            <button role="radio" aria-checked={s.theme === v} onclick={() => { if (s) { s.theme = v as "dark" | "light" | "system"; save(); } }}>{t(k as Parameters<typeof t>[0])}</button>
          {/each}
        </div>
      </div>
      <label class="row check">
        <div class="lbl"><span>{t("set.expanderOn")}</span></div>
        <input type="checkbox" class="switch" bind:checked={s.expander.enabled} onchange={save} />
      </label>
      <div class="row" class:off={!s.expander.enabled}>
        <div class="lbl"><span>{t("set.prefix")}</span><small>{t("set.prefixHelp")}</small></div>
        <input class="field tiny" maxlength="2" bind:value={s.expander.prefix} onchange={save} disabled={!s.expander.enabled} />
      </div>
    </section>

    <section>
      <h2>{t("set.privacy")}</h2>
      <label class="row check">
        <div class="lbl"><span>{t("set.pwm")}</span></div>
        <input type="checkbox" class="switch" bind:checked={s.ignorePasswordManagers} onchange={save} />
      </label>
      <div class="row top">
        <div class="lbl"><span>{t("set.excluded")}</span><small>{t("set.excludedHelp")}</small></div>
        <textarea class="field apps" bind:value={excludedText} onchange={save} spellcheck="false"></textarea>
      </div>
      <div class="row">
        <div class="lbl"><span>{t("set.lockAfter")}</span></div>
        <select class="field narrow" bind:value={s.lockAfterMinutes} onchange={save}>
          {#each [5, 15, 30, 60] as n}<option value={n}>{t("set.lockN", { n })}</option>{/each}
          <option value={0}>{t("set.lockNever")}</option>
        </select>
      </div>
      <label class="row check">
        <div class="lbl"><span>{t("set.pause")}</span></div>
        <input type="checkbox" class="switch" bind:checked={paused} onchange={() => api.historyPause(paused)} />
      </label>
    </section>

    <section>
      <h2>{t("set.security")}</h2>
      <div class="row">
        <div class="lbl"><span>{t("set.hello")}</span><small>{hello.available ? t("set.helloHelp") : t("set.helloUnavailable")}</small></div>
        <input type="checkbox" class="switch" checked={hello.enabled} disabled={!hello.available || helloBusy} onchange={toggleHello} aria-label={t("set.hello")} />
      </div>
      <div class="row">
        <div class="lbl"><span>{t("set.journal")}</span><small>{t("set.journalHelp")}</small></div>
        <button class="btn" onclick={openJournal}><Icon name="clock" />{t("set.journalShow")}</button>
      </div>
    </section>

    <section>
      <h2>{t("set.services")}</h2>
      <p class="note">{t("set.servicesHelp")}</p>
      {#each [["translate", "set.svc.translate"], ["shorten", "set.svc.shorten"]] as [svc, key]}
        <div class="row">
          <div class="lbl"><span>{t(key as Parameters<typeof t>[0])}</span><small>{consents.includes(svc) ? t("set.svc.on") : t("set.svc.off")}</small></div>
          <input type="checkbox" class="switch" checked={consents.includes(svc)} onchange={() => toggleConsent(svc as "translate" | "shorten")} aria-label={t(key as Parameters<typeof t>[0])} />
        </div>
      {/each}
    </section>

    <section>
      <h2>{t("set.legal")}</h2>
      <div class="row">
        <div class="lbl"><span>{t("set.legal")}</span><small>{t("set.legalHelp")}</small></div>
        <div class="btns">
          <button class="btn" onclick={() => (legalOpen = "privacy")}>{t("set.readPrivacy")}</button>
          <button class="btn" onclick={() => (legalOpen = "terms")}>{t("set.readTerms")}</button>
        </div>
      </div>
      {#if !isOwner}
        <div class="row">
          <div class="lbl"><span>{t("set.erase")}</span><small>{t("set.eraseHelp")}</small></div>
          <button class="btn danger" onclick={() => { eraseOpen = true; eraseWord = ""; eraseErr = ""; }}>{t("set.erase")}</button>
        </div>
      {/if}
    </section>

    <section>
      <h2>{t("set.updates")}</h2>
      <div class="row">
        <div class="lbl"><span>{t("set.github")}</span><small>{t("set.githubHelp")}</small></div>
        <button class="btn" onclick={() => api.openGithub()}>GitHub</button>
      </div>
      <div class="row">
        <div class="lbl">
          <span>{t("set.version", { v: VERSION })}</span>
          <small class:bad={!!updates.error}>
            {#if updates.progress !== null}{t("upd.installing", { p: updates.progress })}
            {:else if updates.checking}{t("set.checking")}
            {:else if updates.error}{t("upd.failed", { e: updates.error })}
            {:else if updates.found}{t("upd.available", { v: updates.found.version })}
            {:else if checkedNow || updates.checked}{t("set.upToDate")}
            {:else}{inTauri ? t("set.updatesHelp") : ""}{/if}
          </small>
        </div>
        {#if updates.found}
          <button class="btn primary" onclick={installUpdate} disabled={updates.progress !== null}>{t("upd.install")}</button>
        {:else}
          <button class="btn" onclick={check} disabled={updates.checking}><Icon name="refresh" />{t("set.checkUpdates")}</button>
        {/if}
      </div>
    </section>

    <section>
      <h2>{t("set.data")}</h2>
      <div class="row">
        <div class="lbl"><span>{t("tab.snippets")}</span></div>
        <div class="btns">
          <button class="btn" onclick={exportSnippets}><Icon name="download" />{t("lib.export")}</button>
          <button class="btn" onclick={importSnippets}><Icon name="upload" />{t("lib.import")}</button>
        </div>
      </div>
      <div class="row">
        <div class="lbl"><span>{t("set.backup")}</span><small>{t("set.backupHelp")}</small></div>
        <div class="btns">
          <button class="btn" onclick={() => { backupOpen = "export"; backupPw = ""; backupErr = ""; }}><Icon name="download" />{t("set.backupMake")}</button>
          <button class="btn" onclick={() => { backupOpen = "import"; backupPw = ""; backupErr = ""; }}><Icon name="upload" />{t("set.backupRestore")}</button>
        </div>
      </div>
      <div class="row">
        <div class="lbl"><span>{t("set.clear")}</span><small>{t("set.clearHelp")}</small></div>
        <button class="btn danger" onclick={async () => { if (await ask({ title: t("set.clearQ"), text: t("set.clearHelp"), ok: t("set.clear"), danger: true })) { await api.historyClear(); cleared = true; } }}>{cleared ? t("set.cleared") : t("set.clear")}</button>
      </div>
      <div class="row">
        <div class="lbl"><span>{t("set.lockNow")}</span></div>
        <button class="btn" onclick={() => api.lock()}><Icon name="lock" />{t("set.lockNow")}</button>
      </div>
    </section>
  </div>
{/if}

{#if backupOpen}
  <Modal title={backupOpen === "export" ? t("set.backupMake") : t("set.backupRestore")} onclose={() => (backupOpen = null)} width={440}>
    <form class="erase" onsubmit={runBackup}>
      <label><span>{t("set.backupPw")}</span><input class="field" type="password" bind:value={backupPw} autocomplete="new-password" /></label>
      <p>{t("set.backupPwHelp")}</p>
      {#if backupErr}<p class="bad">{backupErr}</p>{/if}
      <div class="acts">
        <button type="button" class="btn ghost" onclick={() => (backupOpen = null)}>{t("common.cancel")}</button>
        <button class="btn primary" disabled={backupBusy || !backupPw}>{backupOpen === "export" ? t("set.backupMake") : t("set.backupRestore")}</button>
      </div>
    </form>
  </Modal>
{/if}

{#if legalOpen}
  <Modal title={legalOpen === "privacy" ? t("legal.privacy") : t("legal.terms")} onclose={() => (legalOpen = null)} width={720}>
    <LegalView doc={legalOpen} />
  </Modal>
{/if}

{#if eraseOpen}
  <Modal title={t("set.eraseQ")} onclose={() => (eraseOpen = false)} width={460}>
    <form class="erase" onsubmit={erase}>
      <p>{t("set.eraseText")}</p>
      <label><span>{t("set.eraseType")}</span><input class="field" bind:value={eraseWord} autocomplete="off" spellcheck="false" /></label>
      {#if eraseErr}<p class="bad">{eraseErr}</p>{/if}
      <div class="acts">
        <button type="button" class="btn ghost" onclick={() => (eraseOpen = false)}>{t("common.cancel")}</button>
        <button class="btn primary dangerfill" disabled={erasing || eraseWord.trim().toUpperCase() !== t("set.eraseWord")}>{t("set.erase")}</button>
      </div>
    </form>
  </Modal>
{/if}

{#if journalOpen}
  <Modal title={t("set.journal")} onclose={() => (journalOpen = false)} width={520}>
    {#if journal.length}
      <ul class="journal">
        {#each journal as j}
          <li class:bad={j.kind === "unlock_fail"}>
            <span class="jk">{t(jKey(j.kind)) === "j." + j.kind ? j.kind : t(jKey(j.kind))}</span>
            <span class="jd">{j.detail}</span>
            <time>{when(j.at)}</time>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="muted">{t("set.journalEmpty")}</p>
    {/if}
  </Modal>
{/if}

<style>
  .erase { display: grid; gap: 12px; }
  .erase p { margin: 0; line-height: 1.55; font-size: var(--t-s); color: var(--muted); }
  .erase p.bad { color: var(--danger); }
  .erase label { display: grid; gap: 5px; font-size: var(--t-s); color: var(--muted); }
  .erase .acts { display: flex; justify-content: flex-end; gap: 6px; }
  .dangerfill:not(:disabled) { background: var(--danger); border-color: var(--danger); color: #fff; }
  .note { margin: 0 0 4px; color: var(--faint); font-size: var(--t-s); line-height: 1.5; max-width: 60ch; }
  .journal { list-style: none; margin: 0; padding: 0; max-height: 56vh; overflow-y: auto; }
  .journal li { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 2px 12px; padding: 9px 0; border-bottom: 1px solid var(--line); font-size: var(--t-s); }
  .journal li.bad .jk { color: var(--danger); }
  .jd { grid-column: 1; color: var(--faint); font-size: var(--t-xs); }
  .jd:empty { display: none; }
  .journal time { grid-row: 1; grid-column: 2; color: var(--muted); font-size: var(--t-xs); white-space: nowrap; }
  .switch:disabled { opacity: 0.4; cursor: default; }
  .page { overflow-y: auto; height: 100%; padding: 28px clamp(16px, 4vw, 40px) 60px; }
  @media (max-width: 640px) { .row { flex-wrap: wrap; gap: 10px; } }
  section { max-width: 680px; margin-bottom: 36px; }
  h2 { font: 600 var(--t-l)/1.3 var(--font-display); margin: 0 0 6px; }
  .row { display: flex; align-items: center; justify-content: space-between; gap: 24px; padding: 12px 0; border-bottom: 1px solid var(--line); }
  .row.top { align-items: flex-start; }
  .row.off { opacity: 0.45; }
  .row.check { cursor: pointer; }
  .lbl { display: grid; gap: 2px; }
  .lbl small { color: var(--faint); font-size: var(--t-s); max-width: 46ch; }
  .lbl small.bad { color: var(--danger); }
  .seg { display: inline-flex; border: 1px solid var(--line); border-radius: var(--r-s); padding: 2px; background: var(--sunk); }
  .seg button { padding: 4px 12px; border-radius: 3px; color: var(--muted); font-size: var(--t-s); }
  .seg button[aria-checked="true"] { background: var(--text); color: var(--bg); }
  .narrow { width: 180px; }
  .tiny { width: 56px; text-align: center; font-family: var(--mono); }
  .apps { width: 240px; height: 80px; resize: vertical; font-family: var(--mono); font-size: 12px; }
  .btns { display: flex; gap: 6px; }
  .switch { appearance: none; width: 32px; height: 18px; border-radius: 9px; background: var(--line-strong); position: relative; cursor: pointer; flex: none; margin: 0; transition: background 140ms var(--ease); }
  .switch::after { content: ""; position: absolute; top: 3px; left: 3px; width: 12px; height: 12px; border-radius: 50%; background: var(--muted); transition: transform 140ms var(--ease), background 140ms var(--ease); }
  .switch:checked { background: var(--text); }
  .switch:checked::after { transform: translateX(14px); background: var(--bg); }
</style>
