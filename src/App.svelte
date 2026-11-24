<script lang="ts">
  import { onMount } from "svelte";
  import { api, listen, type Status, type PickerMode } from "./lib/api";
  import { i18n } from "./lib/i18n.svelte";
  import Picker from "./views/Picker.svelte";
  import Quick from "./views/Quick.svelte";
  import { applyTheme } from "./lib/theme";
  import { LEGAL_VERSION } from "./lib/version";
  import type { Settings } from "./lib/api";

  // Тяжёлые экраны грузятся только в своих окнах: окно вставки открывается мгновенно.
  const loadMain = () => import("./views/Main.svelte");
  const loadSetup = () => import("./views/Setup.svelte");
  const loadLock = () => import("./views/Lock.svelte");
  const loadPrint = () => import("./views/Print.svelte");
  const loadConsent = () => import("./views/Consent.svelte");

  // Одно приложение обслуживает несколько окон. Какое это окно, говорит адрес.
  const hash = new URLSearchParams(location.hash.slice(1));
  const view = hash.get("view") ?? "main";
  const mode = (hash.get("mode") ?? "popup") as PickerMode;
  const lastSection = (() => { try { return localStorage.getItem("clavis.section"); } catch { return null; } })();
  const section = hash.get("section") ?? lastSection ?? "notes";

  let status = $state<Status | null>(null);
  let settings = $state<Settings | null>(null);
  // Согласие с условиями спрашивается до любых действий, а после новой редакции документов — снова.
  const needConsent = $derived(!!settings && view !== "picker" && view !== "quick" && settings.legalAccepted !== LEGAL_VERSION);

  async function acceptLegal(version: string) {
    if (!settings) return;
    const next = { ...$state.snapshot(settings), legalAccepted: version, lang: i18n.lang };
    await api.settingsSave(next).catch(() => {});
    settings = next;
  }

  async function refresh() {
    status = await api.status();
    const s = await api.settings().catch(() => null);
    if (s) { i18n.lang = s.lang; settings = s; applyTheme(s.theme); }
  }

  onMount(() => {
    if (view === "print") return;
    refresh();
    let off: (() => void) | undefined;
    listen("status:changed", refresh).then((o) => (off = o));
    listen("settings:changed", refresh);
    return () => off?.();
  });
</script>

{#if view === "print"}
  {#await loadPrint() then m}<m.default />{/await}
{:else if needConsent}
  {#await loadConsent() then m}<m.default onaccept={acceptLegal} updated={!!settings?.legalAccepted} />{/await}
{:else if status}
  {#if view === "picker"}
    {#if status.stage === "ready"}<Picker initialMode={mode} />{/if}
  {:else if view === "quick"}
    <Quick />
  {:else if status.stage === "activate" || status.stage === "setup"}
    {#await loadSetup() then m}<m.default stage={status.stage} ondone={refresh} />{/await}
  {:else if status.stage === "locked" || status.stage === "blocked"}
    {#await loadLock() then m}<m.default stage={status.stage} ondone={refresh} />{/await}
  {:else}
    {#await loadMain() then m}<m.default owner={status.owner} initial={section} />{/await}
  {/if}
{/if}
