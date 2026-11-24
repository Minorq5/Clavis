<script lang="ts">
  import { onMount } from "svelte";
  import { api, type AccessRequest, type Device, type IssuedCode } from "../lib/api";
  import { t, ago } from "../lib/i18n.svelte";
  import Icon from "../lib/Icon.svelte";
  import { toast } from "../lib/toast.svelte";

  let requests = $state<AccessRequest[]>([]);
  let codes = $state<IssuedCode[]>([]);
  let devices = $state<Device[]>([]);
  let label = $state("");
  let issued = $state<{ label: string; code: string } | null>(null);
  let copied = $state(false);

  let error = $state("");
  async function load() {
    try {
      [requests, codes, devices] = await Promise.all([api.ownerRequests(), api.ownerCodes(), api.ownerDevices()]);
      error = "";
    } catch (e) {
      error = String(e) === "network" ? t("setup.netError") : t("setup.serverError");
    }
  }
  onMount(load);

  async function issue(forLabel: string, requestId?: string) {
    const code = await api.ownerIssue(forLabel, requestId);
    issued = { label: forLabel, code };
    copied = false;
    label = "";
    await load();
  }

  async function copy() {
    if (!issued) return;
    await api.copyText(issued.code);
    copied = true;
  }
</script>

{#snippet meta(pairs: [string, string | undefined][])}
  {#if pairs.some(([, v]) => v)}
    <span class="meta">
      {#each pairs.filter(([, v]) => v) as [k, v]}
        <button class="mv" title={t("common.copy")} onclick={() => api.copyText(v ?? "").then(() => toast(t("common.copied")))}><i>{k}</i>{v}</button>
      {/each}
    </span>
  {/if}
{/snippet}

<div class="page">
  {#if error}<p class="err">{error}</p>{/if}
  {#if issued}
    <div class="issued">
      <p>{t("owner.issued")}</p>
      <div class="code"><span>{issued.code}</span><button class="btn" onclick={copy}><Icon name={copied ? "check" : "copy"} />{copied ? t("common.copied") : t("common.copy")}</button></div>
      <button class="btn ghost close" onclick={() => (issued = null)} aria-label={t("common.cancel")}><Icon name="close" /></button>
    </div>
  {/if}

  <section>
    <h2>{t("owner.requests")}</h2>
    {#each requests as r (r.id)}
      <div class="row">
        <div class="lbl"><span>{r.name}</span><small>{r.message || "—"}</small>{@render meta([[t("owner.ip"), r.ip], [t("owner.hw"), r.hw]])}</div>
        <span class="when faint">{ago(r.at)}</span>
        <div class="btns">
          <button class="btn ghost" onclick={async () => { await api.ownerDecline(r.id); load(); }}>{t("owner.decline")}</button>
          <button class="btn primary" onclick={() => issue(r.name, r.id)}>{t("owner.approve")}</button>
        </div>
      </div>
    {:else}
      <p class="none">{t("owner.noRequests")}</p>
    {/each}
  </section>

  <section>
    <h2>{t("owner.codes")}</h2>
    <form class="new" onsubmit={(e) => { e.preventDefault(); if (label.trim()) issue(label.trim()); }}>
      <input class="field" bind:value={label} placeholder={t("owner.issueLabel")} />
      <button class="btn primary" disabled={!label.trim()}><Icon name="key" />{t("owner.issue")}</button>
    </form>
    {#each codes as c (c.id)}
      <div class="row">
        <div class="lbl"><span>{c.label}</span><small>{c.used ? t("owner.used") : t("owner.unused")}</small></div>
        <span class="when faint">{ago(c.created)}</span>
        <div class="btns"><button class="btn ghost danger" onclick={async () => { await api.ownerRevokeCode(c.id); load(); }}>{t("owner.revoke")}</button></div>
      </div>
    {/each}
  </section>

  <section>
    <h2>{t("owner.devices")}</h2>
    {#each devices as d (d.id)}
      <div class="row">
        <div class="lbl">
          <span>{d.name}</span>
          <small class:bad={d.blocked}>{d.blocked ? t("owner.blocked") : t("owner.active", { t: ago(d.lastSeen) })}</small>
          {@render meta([[t("owner.ip"), d.ip], [t("owner.computer"), d.computer], [t("owner.shown"), d.displayName], [t("owner.hw"), d.hw]])}
        </div>
        <div class="btns">
          {#if d.blocked}
            <button class="btn" onclick={async () => { await api.ownerUnblock(d.id); load(); }}>{t("owner.unblock")}</button>
          {:else}
            <button class="btn ghost danger" onclick={async () => { await api.ownerRevokeDevice(d.id); load(); }}>{t("owner.revoke")}</button>
          {/if}
        </div>
      </div>
    {/each}
  </section>
</div>

<style>
  .page { overflow-y: auto; height: 100%; padding: 28px clamp(16px, 4vw, 40px) 60px; }
  section { max-width: 720px; margin-bottom: 36px; }
  h2 { font: 600 var(--t-l)/1.3 var(--font-display); margin: 0 0 6px; }
  .row { display: grid; grid-template-columns: 1fr auto auto; align-items: center; gap: 20px; padding: 11px 0; border-bottom: 1px solid var(--line); }
  .lbl { display: grid; gap: 1px; min-width: 0; }
  .lbl small { color: var(--faint); font-size: var(--t-s); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .lbl small.bad { color: var(--danger); }
  .when { font-size: var(--t-xs); }
  .meta { display: flex; flex-wrap: wrap; gap: 4px 12px; margin-top: 4px; }
  .mv { font: 11.5px/1.4 var(--mono); color: var(--muted); padding: 0; user-select: text; }
  .mv i { font-style: normal; color: var(--faint); margin-right: 5px; font-family: var(--font); }
  .mv:hover { color: var(--text); }
  .btns { display: flex; gap: 6px; }
  .none { color: var(--muted); margin: 10px 0; }
  .new { display: flex; gap: 8px; margin: 8px 0 6px; }
  .issued { position: relative; max-width: 720px; margin-bottom: 32px; padding: 16px 18px; border: 1px solid var(--text); border-radius: var(--r-m); }
  .issued p { margin: 0 0 12px; max-width: 56ch; color: var(--muted); }
  .code { display: flex; align-items: center; gap: 16px; }
  .code span { font: 500 19px/1 var(--mono); letter-spacing: 0.06em; user-select: all; }
  .err { color: var(--danger); margin: 0 0 20px; }
  .close { position: absolute; top: 8px; right: 8px; padding: 0 6px; }
</style>
